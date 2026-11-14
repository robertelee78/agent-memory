// Native stored-source transport only. Canonical operations remain approved tools.
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { closeSync, constants, fstatSync, lstatSync, openSync, readSync, realpathSync, type Stats } from 'node:fs';
import { dirname, isAbsolute, join, parse, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import type { ExtensionAPI, ExtensionContext } from '@oh-my-pi/pi-coding-agent';

type Binding = { artifact: string; executable: string; agentDir: string; rolloutAttested: true; readerCapability: 'omp-stored-source-v2' };
type NativeEntry = { type: string; id: string; parentId: string | null; message?: Record<string, unknown>;
  customType?: string; content?: unknown; attribution?: string; timestamp?: string; agent?: { kind?: string } };
type Header = { id: string; cwd: string; version: number; parent_session: string | null; timestamp: string | null };
type View = { session: string; cwd: string; file: string | null; header: Header; leaf_id: string | null;
  native_parents: { id: string; parent_id: string | null; timestamp: string | null }[]; source_kind: 'session-manager' | 'retired-file';
  tail_status: 'available' | 'active-unavailable'; fork_status: 'none' | 'verified' | 'unavailable' };
type Identity = { view_id: string; leaf_id: string | null; native_ids: string[]; tool_ids: string[] };
type Payload = { entries: Record<string, unknown>[]; branch_ids: string[]; event_ordinal: number;
  terminal: boolean; trigger: string; inherited_ids: string[]; view: View; request_wake?: boolean };
type StoredSource = { entries: Record<string, unknown>[]; branch_ids: string[]; view: View };
type LastView = { view: View; identity: Identity | null; branch_ids: string[] };
type Result = { verified?: boolean; status?: string; continue?: boolean; additionalContext?: string; request_id?: string; source_coverage_complete?: boolean };
type Pending = { bytes: number; received: boolean; resolve: (value: Result | undefined) => void };
type State = { session: string; repo: string; lifecycle: string; instance: string; key: string;
  child: ChildProcessWithoutNullStreams; ordinal: number; sequence: number; refused: number; retiredThrough: number;
  bytes: number; pending: Map<number, Pending>; ready: boolean; broken: boolean; closing: boolean; retiring: boolean;
  startup: Promise<boolean>; startupResolve: (ready: boolean) => void;
  flushed: Promise<boolean>; flushResolve: (flushed: boolean) => void;
  inherited: Set<string>; request?: string; human?: string; activeRun: boolean;
  last?: LastView; nonce?: string;
  clock?: { nonce: string; resolve: (value: { p1: string; p2: string; j3: bigint } | undefined) => void } };
class CaptureRefusal extends Error {
  constructor(readonly code: string) { super('Native stored view refused'); }
}
const REGISTRY = Symbol.for('agent-memory.omp.native-observer.v2');
const globals = globalThis as typeof globalThis & { [key: symbol]: Map<string, { token: symbol; artifact: string }> | undefined };
const registry = globals[REGISTRY] ??= new Map();
const MAX_INPUT = 16 * 1024 * 1024;
const MAX_QUEUE = 64 * 1024 * 1024;
const MAX_FRAMES = 4096;
const MAX_IDENTITIES = 16384;
// Provisional bounded cap; host-loop blocking/support still requires platform qualification.
const RETIRED_FILE_CAP = MAX_INPUT;
const forbidden: Record<string, true> = { providerPayload: true, provider_payload: true,
  thinking: true, reasoning: true, thinkingSignature: true, textSignature: true,
  signature: true, auth: true, credentials: true, credential: true, config: true, contextSnapshot: true };
function learningTree(value: unknown): unknown {
  if (value === null || typeof value !== 'object') return value;
  if (Array.isArray(value)) return value.map(learningTree);
  if ('type' in value && ['image', 'image_url', 'input_image'].includes(String(value.type))) return { type: 'unavailable' };
  if (Object.keys(value).some(key => forbidden[key])) return { type: 'unavailable' };
  const result: Record<string, unknown> = {};
  for (const [key, child] of Object.entries(value)) result[key] = learningTree(child);
  return result;
}
function learningContent(value: unknown): unknown {
  if (!Array.isArray(value)) return typeof value === 'string' ? value : null;
  return value.flatMap(block => {
    if (!block || typeof block !== 'object' || !('type' in block)) return [{ type: 'unavailable' }];
    if (['thinking', 'reasoning', 'redactedThinking'].includes(String(block.type))) return [];
    if (block.type === 'text' && 'text' in block) return [{ type: 'text', text: block.text }];
    if (block.type === 'toolCall' && 'id' in block && 'name' in block && 'arguments' in block)
      return [{ type: 'toolCall', id: block.id, name: block.name, arguments: learningTree(block.arguments) }];
    return [{ type: 'unavailable' }];
  });
}
function storedEntries(entries: readonly NativeEntry[]) {
  return entries.map(entry => {
    const carrier: Record<string, unknown> = { type: entry.type, id: entry.id, parentId: entry.parentId };
    if (entry.agent?.kind === 'sub') carrier.agent = { kind: 'sub' };
    if (entry.type === 'message') {
      const native = entry.message;
      if (!native) throw new CaptureRefusal('unsafe-view');
      const message: Record<string, unknown> = { role: native.role };
      for (const [key, child] of Object.entries(native)) {
        if (key === 'content') message.content = learningContent(child);
        else if (['attribution', 'toolCallId', 'toolName', 'isError', 'command', 'code', 'output',
          'exitCode', 'cancelled', 'truncated', 'excludeFromContext', 'customType'].includes(key)) message[key] = child;
        else if (key === 'details' || key === 'files') message[key] = learningTree(child);
        else if (key === 'images') message.images = [{ type: 'unavailable' }];
      }
      carrier.message = message;
    } else if (entry.type === 'custom_message') {
      carrier.customType = entry.customType; carrier.content = learningContent(entry.content);
      if (entry.attribution !== undefined) carrier.attribution = entry.attribution;
    }
    return carrier;
  });
}
function nativeId(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && Buffer.byteLength(value) <= 512 && !/[\x00-\x1f]/.test(value);
}
function graph(entries: readonly NativeEntry[], leaf: string | null) {
  if (entries.length > MAX_IDENTITIES) throw new CaptureRefusal('oversized-source');
  const parents = new Map<string, string | null>();
  for (const entry of entries) {
    if (!nativeId(entry.id) || /^(?:event|tool):/.test(entry.id)
      || (entry.parentId !== null && !nativeId(entry.parentId)) || parents.has(entry.id)) throw new CaptureRefusal('unsafe-view');
    parents.set(entry.id, entry.parentId);
  }
  const finished = new Set<string>();
  for (const id of parents.keys()) {
    const chain = new Set<string>(); let current: string | null = id;
    while (current !== null && !finished.has(current)) {
      if (!parents.has(current) || chain.has(current)) throw new CaptureRefusal('unsafe-view');
      chain.add(current); current = parents.get(current)!;
    }
    for (const item of chain) finished.add(item);
  }
  if (leaf !== null && !parents.has(leaf)) throw new CaptureRefusal('unsafe-view');
  const branch: string[] = []; let current = leaf;
  while (current !== null) { branch.push(current); current = parents.get(current)!; }
  return { parents: [...parents].map(([id, parent_id]) => ({ id, parent_id })), branch: branch.reverse() };
}
function sourceTools(entries: Record<string, unknown>[]) {
  const ids = new Set<string>();
  for (const entry of entries) {
    const message = entry.message as Record<string, unknown> | undefined;
    if (message?.role === 'toolResult' && nativeId(message.toolCallId)) ids.add(message.toolCallId);
    if (message?.role === 'assistant' && Array.isArray(message.content))
      for (const block of message.content) if (block?.type === 'toolCall' && nativeId(block.id)) ids.add(block.id);
  }
  return [...ids].sort();
}
function same(left: unknown, right: unknown) { return JSON.stringify(left) === JSON.stringify(right); }
function storedTimestamp(value: unknown) {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?Z$/.test(value)) return null;
  const stamp = Date.parse(value);
  if (!Number.isFinite(stamp)) return null;
  const normalized = new Date(stamp).toISOString();
  if (normalized.slice(0, 19) !== value.slice(0, 19)) return null;
  const fraction = value.includes('.') ? value.slice(value.indexOf('.') + 1, -1) : '';
  return value.slice(0, 19) + '.' + fraction.padEnd(6, '0') + 'Z';
}
function forkStatus(header: Header, parents: View['native_parents']): View['fork_status'] {
  return !header.parent_session ? 'none' : header.timestamp && parents.every(entry => entry.timestamp) ? 'verified' : 'unavailable';
}
function headerValue(header: { id?: string; cwd?: string; version?: number; parentSession?: string; timestamp?: string } | null | undefined): Header {
  if (!header || !nativeId(header.id) || header.version !== 3 || typeof header.cwd !== 'string' || !isAbsolute(header.cwd)) throw new CaptureRefusal('unsupported-view');
  return { id: header.id, cwd: header.cwd, version: header.version, parent_session: header.parentSession ?? null, timestamp: storedTimestamp(header.timestamp) };
}
function checkedStoredView(ctx: ExtensionContext, tail: View['tail_status'] = 'available') {
  const manager = ctx.sessionManager;
  const binding = () => ({ session: manager.getSessionId(), cwd: manager.getCwd(), file: manager.getSessionFile() ?? null,
    header: headerValue(manager.getHeader()), leaf_id: manager.getLeafId() });
  const before = binding();
  const native = manager.getEntries() as unknown as NativeEntry[];
  const tree = graph(native, before.leaf_id);
  const branch = manager.getBranch().map(entry => entry.id);
  const entries = storedEntries(native);
  const after = binding();
  if (!same(before, after) || before.session !== before.header.id || !same(branch, tree.branch)) throw new CaptureRefusal('identity-changed');
  const timestamps = new Map(native.map(entry => [entry.id, storedTimestamp(entry.timestamp)]));
  const parents = tree.parents.map(entry => ({ ...entry, timestamp: timestamps.get(entry.id) ?? null }));
  const view: View = { ...before, native_parents: parents, source_kind: 'session-manager', tail_status: tail, fork_status: forkStatus(before.header, parents) };
  return { view, entries, branch_ids: branch };
}
function canonicalFile(file: string) {
  if (!isAbsolute(file) || file.split('/').includes('..')) throw new CaptureRefusal('unsafe-view');
  const parts = file.split('/').filter(Boolean); let current = parse(file).root;
  for (const part of parts) {
    current = join(current, part); const info = lstatSync(current);
    if (info.isSymbolicLink()) {
      if (info.uid !== 0 || !['/tmp', '/var', '/etc'].includes(current)) throw new CaptureRefusal('unsafe-view');
      current = realpathSync(current);
    }
  }
  return current;
}
function trustedParents(file: string) {
  const paths: string[] = []; let current = dirname(file);
  while (true) { paths.push(current); const parent = dirname(current); if (parent === current) break; current = parent; }
  let protectedPath = false;
  return paths.reverse().map(path => {
    const stat = lstatSync(path); const uid = process.getuid?.();
    if (!stat.isDirectory() || stat.isSymbolicLink() || (stat.uid !== 0 && stat.uid !== uid)) throw new CaptureRefusal('unsafe-view');
    if (!protectedPath && (stat.mode & 0o022) && !(stat.uid === 0 && (stat.mode & 0o1000))) throw new CaptureRefusal('unsafe-view');
    if (stat.uid === uid && !(stat.mode & 0o077)) protectedPath = true;
    return { path, dev: stat.dev, ino: stat.ino, uid: stat.uid, mode: stat.mode };
  });
}
function retiredView(pi: ExtensionAPI, old: LastView): StoredSource {
  if (!old.view.file) throw new CaptureRefusal('missing-payload');
  const file = canonicalFile(old.view.file); const parents = trustedParents(file);
  const descriptor = openSync(file, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const metadata = (stat: Stats) => ({ dev: stat.dev, ino: stat.ino, size: stat.size,
      mtime: stat.mtimeMs, ctime: stat.ctimeMs, uid: stat.uid, mode: stat.mode, nlink: stat.nlink });
    const first = fstatSync(descriptor), named = lstatSync(file);
    if (!first.isFile() || first.uid !== process.getuid?.() || first.nlink !== 1 || (first.mode & 0o022)
      || first.size > RETIRED_FILE_CAP || !same(metadata(first), metadata(named))) throw new CaptureRefusal('unsafe-view');
    const bytes = Buffer.alloc(first.size); let count = 0;
    while (count < bytes.length) { const read = readSync(descriptor, bytes, count, Math.min(65536, bytes.length - count), null); if (!read) throw new CaptureRefusal('missing-payload'); count += read; }
    if (readSync(descriptor, Buffer.alloc(1), 0, 1, null)) throw new CaptureRefusal('identity-changed');
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    const parsed = pi.pi.parseSessionContent(text);
    if (!same(Object.keys(parsed).sort(), ['entries', 'invalidHeader', 'malformedRecords', 'sourceSize', 'titleSlot'])
      || parsed.invalidHeader !== false || parsed.malformedRecords !== 0 || parsed.sourceSize !== bytes.length
      || !Array.isArray(parsed.entries)) throw new CaptureRefusal('unsafe-view');
    const headers = parsed.entries.filter(entry => entry.type === 'session');
    if (headers.length !== 1 || parsed.entries[0] !== headers[0]) throw new CaptureRefusal('unsafe-view');
    const header = headerValue(headers[0]);
    if (header.id !== old.view.session || resolve(header.cwd) !== resolve(old.view.header.cwd)
      || header.version !== pi.pi.CURRENT_SESSION_VERSION || header.timestamp !== old.view.header.timestamp
      || header.parent_session !== old.view.header.parent_session) throw new CaptureRefusal('identity-changed');
    const native = parsed.entries.slice(1) as unknown as NativeEntry[];
    // The selector handles activeLeafId, but deliberately does not validate graphs.
    graph(native, null);
    const leaf = pi.pi.resolveSessionLeafId(parsed.entries.slice(1));
    const tree = graph(native, leaf);
    const identities = new Map(tree.parents.map(entry => [entry.id, entry.parent_id]));
    if (old.view.native_parents.some(entry => !identities.has(entry.id) || identities.get(entry.id) !== entry.parent_id)) throw new CaptureRefusal('identity-changed');
    const entries = storedEntries(native);
    if (!same(metadata(first), metadata(fstatSync(descriptor))) || !same(metadata(first), metadata(lstatSync(file)))
      || !same(parents, trustedParents(file)) || canonicalFile(old.view.file) !== file) throw new CaptureRefusal('identity-changed');
    const timestamps = new Map(native.map(entry => [entry.id, storedTimestamp(entry.timestamp)]));
    const recoveredParents = tree.parents.map(entry => ({ ...entry, timestamp: timestamps.get(entry.id) ?? null }));
    if (old.view.native_parents.some(entry => timestamps.get(entry.id) !== entry.timestamp)) throw new CaptureRefusal('identity-changed');
    const view: View = { ...old.view, file, header, leaf_id: leaf, native_parents: recoveredParents,
      source_kind: 'retired-file', tail_status: 'available', fork_status: forkStatus(header, recoveredParents) };
    return { view, entries, branch_ids: tree.branch };
  } finally { closeSync(descriptor); }
}

export default function nativeMemory(pi: ExtensionAPI, binding?: Binding) {
  const token = Symbol('agent-memory-module'); const states = new Map<string, State>(); const instances = new Set<State>(); let active: State | undefined;
  let diagnosed = false; let shuttingDown = false; let hintTimer: NodeJS.Timeout | undefined; let hintContext: ExtensionContext | undefined; let lastHintSweep = 0;
  const warn = (ctx: ExtensionContext, code: string) => {
    ctx.ui.setStatus('agent-memory', 'am: ' + code);
    if (!diagnosed) { diagnosed = true; ctx.ui.notify('Agent Memory: ' + code + '; sources remain pending. Use am status/doctor and am omp-boundary --list-pending.', 'warning'); }
  };
  const eligible = (ctx: ExtensionContext) => ctx.agent.kind !== 'sub';
  const supported = () => (pi.pi.VERSION === '18.16.0' || pi.pi.VERSION === '18.19.0') && binding?.rolloutAttested === true
    && binding.readerCapability === 'omp-stored-source-v2' && /^[0-9a-f]{64}$/.test(binding.artifact)
    && typeof binding.executable === 'string' && isAbsolute(binding.executable)
    && typeof pi.pi.parseSessionContent === 'function' && pi.pi.CURRENT_SESSION_VERSION === 3
    && typeof pi.pi.resolveSessionLeafId === 'function' && typeof pi.pi.sessionEntryLeafId === 'function'
    && typeof pi.pi.lastSelfSelectedLeafId === 'function';
  const control = (state: State, kind: string, body: unknown) => {
    if (state.broken || state.child.stdin.destroyed) return false;
    const raw = Buffer.from(JSON.stringify(body));
    if (raw.length > 4096 || state.child.stdin.writableLength + raw.length + 256 > MAX_QUEUE) return false;
    state.child.stdin.write(Buffer.concat([Buffer.from(`AM249 ${kind} 0 ${raw.length} -\n`), raw])); return true;
  };
  const cancel = (state: State) => {
    if (state.nonce) control(state, 'CANCEL', { nonce: state.nonce });
    state.nonce = undefined; state.clock?.resolve(undefined); state.clock = undefined;
  };
  const broken = (state: State, ctx: ExtensionContext) => {
    cancel(state); state.broken = true; for (const pending of state.pending.values()) pending.resolve(undefined);
    state.startupResolve(false);
    state.pending.clear(); state.bytes = 0; warn(ctx, 'observer pending');
    // EOF drains already accepted work; never signal a shared-write helper.
    if (!state.child.stdin.destroyed && !state.child.stdin.writableEnded) state.child.stdin.end();
    state.child.stdout.destroy();
  };
  const get = (ctx: ExtensionContext): State | undefined => {
    if (shuttingDown) return;
    if (!eligible(ctx)) return;
    if (!supported()) { warn(ctx, 'inactive: unsupported public API or activation binding'); return; }
    const session = ctx.sessionManager.getSessionId(); const repo = realpathSync(ctx.sessionManager.getCwd());
    const key = JSON.stringify([session, repo, 'agent-memory-native-observer-v2']);
    const owner = registry.get(key);
    if (owner && owner.token !== token) { warn(ctx, 'duplicate-binding'); return; }
    let state = states.get(key); if (state && !state.retiring) return state;
    registry.set(key, { token, artifact: binding!.artifact });
    const instance = randomUUID(), lifecycle = randomUUID();
    const child = spawn(binding!.executable, ['omp-boundary', '--stream', '--repo', repo, '--session', session,
      '--instance', instance, '--lifecycle', lifecycle, '--artifact', binding!.artifact, '--omp-agent-dir', binding!.agentDir],
      { stdio: ['pipe', 'pipe', 'pipe'], cwd: repo, detached: true });
    const { promise: startup, resolve: startupResolve } = Promise.withResolvers<boolean>();
    const { promise: flushed, resolve: flushResolve } = Promise.withResolvers<boolean>();
    state = { session, repo, lifecycle, instance, key, child, ordinal: 0, sequence: 0, refused: 0, retiredThrough: 0,
      bytes: 0, pending: new Map(), ready: false, broken: false, closing: false, retiring: false, inherited: new Set(), activeRun: false, startup, startupResolve, flushed, flushResolve };
    const current = state; states.set(key, current); instances.add(current); child.stderr.resume();
    child.stdin.once('finish', () => current.flushResolve(true));
    child.stdin.once('error', () => current.flushResolve(false));
    let input = Buffer.alloc(0); let expected: { kind: string; seq: number; length: number } | undefined;
    child.stdout.on('data', (chunk: Buffer) => {
      if (current.broken) return;
      if (input.length + chunk.length > 4 * (65536 + 256)) { broken(current, ctx); return; }
      input = Buffer.concat([input, chunk]);
      try {
        while (input.length) {
          if (!expected) {
            const end = input.indexOf(10); if (end < 0) { if (input.length > 256) throw new CaptureRefusal('header-bound'); break; }
            if (end >= 256 || input.subarray(0, end + 1).some(byte => byte > 127)) throw new CaptureRefusal('header-bound');
            const match = /^AM249 (READY|INACTIVE|REFUSED|RECEIVED|OBSERVED|CLOCK-REPLY) (0|[1-9][0-9]{0,15}) (0|[1-9][0-9]{0,8}) -\n$/.exec(input.subarray(0, end + 1).toString('ascii'));
            if (!match || Number(match[3]) > 65536 || !Number.isSafeInteger(Number(match[2]))) throw new CaptureRefusal('response-frame');
            expected = { kind: match[1], seq: Number(match[2]), length: Number(match[3]) }; input = input.subarray(end + 1);
          }
          if (input.length < expected.length) break;
          const j3 = process.hrtime.bigint(); const body = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(input.subarray(0, expected.length)));
          input = input.subarray(expected.length); const frame = expected; expected = undefined;
          if (body.instance_id !== current.instance) throw new CaptureRefusal('response-instance');
          if (frame.kind === 'READY') { current.ready = true; current.startupResolve(true); }
          else if (frame.kind === 'REFUSED' && frame.seq === 0 && body.code === 'response-pressure') {
            for (const pending of current.pending.values()) pending.resolve(undefined);
            warn(ctx, 'observer result unavailable');
          }
          else if (frame.kind === 'INACTIVE' || (frame.kind === 'REFUSED' && frame.seq === 0)) { broken(current, ctx); return; }
          else if (frame.kind === 'CLOCK-REPLY') {
            if (current.clock?.nonce === body.nonce && /^\d{1,24}$/.test(body.p1) && /^\d{1,24}$/.test(body.p2)) current.clock.resolve({ p1: body.p1, p2: body.p2, j3 });
          } else {
            if (frame.seq < 1 || frame.seq > current.sequence) throw new CaptureRefusal('response-sequence');
            // A later FIFO executor response proves earlier bytes were consumed,
            // not that their missing results were delivered or sources completed.
            for (const [sequence, earlier] of current.pending) if (sequence < frame.seq) {
              if (!earlier.received) current.bytes -= earlier.bytes;
              earlier.resolve(undefined); current.pending.delete(sequence);
              current.retiredThrough = Math.max(current.retiredThrough, sequence);
            }
            const pending = current.pending.get(frame.seq);
            if (!pending && frame.seq <= current.retiredThrough) continue;
            if (!pending) throw new CaptureRefusal('response-sequence');
            if (!pending.received) { current.bytes -= pending.bytes; pending.received = true; }
            if (frame.kind === 'OBSERVED' || frame.kind === 'REFUSED') {
              current.pending.delete(frame.seq);
              current.retiredThrough = Math.max(current.retiredThrough, frame.seq);
              if (body.result?.verified === true) pending.resolve(body.result as Result);
              else { pending.resolve(undefined); warn(ctx, 'observer pending'); }
            }
          }
        }
      } catch { broken(current, ctx); }
    });
    child.stdin.on('error', () => broken(current, ctx)); child.once('error', () => broken(current, ctx));
    child.once('close', () => {
      if (current.pending.size || input.length || expected) broken(current, ctx);
      current.startupResolve(false);
      cancel(current); instances.delete(current);
      if (states.get(key) === current) { if (registry.get(key)?.token === token) registry.delete(key); states.delete(key); }
    });
    return current;
  };
  const submit = (state: State, ctx: ExtensionContext, action: 'ingest' | 'seal',
    copy: () => StoredSource, terminal = false, requestWake = false, nonce?: string, updateState = true, shutdownCapture = false) => {
    if (shuttingDown && !shutdownCapture) return { promise: Promise.resolve(undefined), payload: undefined };
    const sequence = ++state.sequence; const ordinal = ++state.ordinal;
    const { promise, resolve: accept } = Promise.withResolvers<Result | undefined>();
    let identity: Identity | null = null; let payload: Payload | undefined; let reason = 'capture-refused'; let raw: Buffer; let accepted = false;
    try {
      if (state.closing || state.broken) throw new CaptureRefusal('startup-pending');
      const source = copy();
      if (source.view.session !== state.session || realpathSync(source.view.cwd) !== state.repo) throw new CaptureRefusal('identity-changed');
      const ids = source.view.native_parents.map(entry => entry.id).sort();
      identity = { view_id: randomUUID(), leaf_id: source.view.leaf_id, native_ids: ids, tool_ids: sourceTools(source.entries) };
      if (identity.tool_ids.length > MAX_IDENTITIES || Buffer.byteLength(JSON.stringify(identity)) > 1024 * 1024) identity = null;
      const header = source.view.header;
      state.inherited.clear();
      if (source.view.fork_status === 'verified') for (const entry of source.view.native_parents)
        if (entry.timestamp && header.timestamp && entry.timestamp <= header.timestamp) state.inherited.add(entry.id);
      const humans = new Set<string>(); const inherited = [...state.inherited];
      for (const entry of source.entries) {
        const message = entry.message as Record<string, unknown> | undefined;
        if (typeof entry.id === 'string' && message?.role === 'user' && message.attribution === 'user') humans.add(entry.id);
      }
      const human = [...source.branch_ids].reverse().find(id => !state.inherited.has(id) && humans.has(id));
      if (updateState && human && human !== state.human) { cancel(state); state.human = human; state.request = undefined; }
      if (updateState && source.view.source_kind === 'session-manager') state.last = { view: source.view, identity, branch_ids: source.branch_ids };
      payload = { ...source, event_ordinal: ordinal, terminal, trigger: state.request ? 'own-hidden-ambiguous' : 'resume', inherited_ids: inherited };
      if (action === 'seal') payload.request_wake = requestWake;
      raw = Buffer.from(JSON.stringify({ request: { schema: 'agent-memory/omp-observer-request-v2', action,
        session: state.session, lifecycle: state.lifecycle, payload }, event_ordinal: ordinal, identity }));
      if (raw.length > MAX_INPUT) throw new CaptureRefusal('oversized-source');
      if (state.bytes + raw.length + 256 > MAX_QUEUE || state.pending.size >= MAX_FRAMES) throw new CaptureRefusal('queue-full');
      accepted = true;
    } catch (error) {
      state.refused++;
      const codes = ['capture-refused', 'identity-changed', 'unsafe-view', 'oversized-source', 'queue-full', 'serialization-failed', 'startup-pending', 'unsupported-view', 'missing-payload'];
      reason = error instanceof CaptureRefusal && codes.includes(error.code) ? error.code : 'capture-refused';
      payload = undefined;
      raw = Buffer.from(JSON.stringify({ action, event_ordinal: ordinal, reason, identity }));
      if (state.broken || state.closing || state.bytes + raw.length + 256 > MAX_QUEUE || state.pending.size >= MAX_FRAMES) {
        accept(undefined); broken(state, ctx); return { promise, payload: undefined };
      }
    }
    const header = Buffer.from(`AM249 ${accepted ? 'REQUEST' : 'REFUSED-ATTEMPT'} ${sequence} ${raw.length} ${accepted && nonce ? nonce : '-'}\n`);
    state.pending.set(sequence, { bytes: raw.length + header.length, received: false, resolve: accept }); state.bytes += raw.length + header.length;
    state.child.stdin.write(Buffer.concat([header, raw]));
    return { promise, payload: accepted ? payload : undefined };
  };
  const capture = (ctx: ExtensionContext, action: 'ingest' | 'seal', terminal = false, requestWake = false, nonce?: string) => {
    const state = get(ctx); if (!state) return;
    const result = { state, ...submit(state, ctx, action, () => checkedStoredView(ctx), terminal, requestWake, nonce) };
    if (active && active !== state) transition(active, state, ctx);
    active = state;
    return result;
  };
  const hint = (ctx: ExtensionContext) => {
    const state = get(ctx); if (!state) return;
    const sequence = ++state.sequence; const ordinal = ++state.ordinal;
    const { promise, resolve: accept } = Promise.withResolvers<Result | undefined>(); void promise;
    let body: unknown; let kind = 'REQUEST';
    try {
      const checked = () => ({ session: ctx.sessionManager.getSessionId(), header_id: ctx.sessionManager.getHeader()?.id,
        cwd: ctx.sessionManager.getCwd(), file: ctx.sessionManager.getSessionFile() ?? null, leaf_id: ctx.sessionManager.getLeafId() });
      const before = checked(), after = checked();
      if (!same(before, after) || before.session !== state.session || before.header_id !== state.session
        || realpathSync(before.cwd) !== state.repo || (before.leaf_id !== null && !nativeId(before.leaf_id))) throw new CaptureRefusal('identity-changed');
      const payload = { session: before.session, header_id: before.header_id, cwd: before.cwd, leaf_id: before.leaf_id };
      body = { request: { schema: 'agent-memory/omp-observer-request-v2', action: 'hint',
        session: state.session, lifecycle: state.lifecycle, payload }, event_ordinal: ordinal, identity: { kind: 'hint', ...payload } };
    } catch {
      kind = 'REFUSED-ATTEMPT'; state.refused++;
      body = { action: 'hint', event_ordinal: ordinal, reason: 'identity-changed', identity: null };
    }
    const raw = Buffer.from(JSON.stringify(body));
    const header = Buffer.from(`AM249 ${kind} ${sequence} ${raw.length} -\n`);
    if (state.broken || state.closing || state.bytes + raw.length + header.length > MAX_QUEUE || state.pending.size >= MAX_FRAMES) {
      if (kind !== 'REFUSED-ATTEMPT') state.refused++;
      broken(state, ctx); return;
    }
    state.pending.set(sequence, { bytes: raw.length + header.length, received: false, resolve: accept });
    state.bytes += raw.length + header.length; state.child.stdin.write(Buffer.concat([header, raw]));
    // One trailing macrotask captures the actual current stored view, never an
    // event body or the assumed origin of any coalesced notification.
    hintContext = ctx;
    if (!hintTimer && !shuttingDown) {
      const elapsed = Number(process.hrtime.bigint()) / 1e6 - lastHintSweep;
      hintTimer = setTimeout(() => {
        hintTimer = undefined; const current = hintContext; hintContext = undefined;
        if (shuttingDown || !current) return;
        lastHintSweep = Number(process.hrtime.bigint()) / 1e6; capture(current, 'ingest');
      }, Math.max(0, 1000 - elapsed));
    }
  };
  const bounded = async <T>(work: Promise<T>, deadline = process.hrtime.bigint() + 950000000n, signal?: AbortSignal): Promise<T | undefined> => {
    const { promise: stopped, resolve: stop } = Promise.withResolvers<undefined>();
    const remaining = Math.max(0, Number(deadline - process.hrtime.bigint()) / 1e6);
    const timer = setTimeout(() => stop(undefined), remaining); const abort = () => stop(undefined);
    if (signal?.aborted || remaining === 0) stop(undefined); else signal?.addEventListener('abort', abort, { once: true });
    try { return await Promise.race([work, stopped]); }
    finally { clearTimeout(timer); signal?.removeEventListener('abort', abort); }
  };
  const finish = (state: State, transition: unknown = null) => {
    if (state.closing) return; cancel(state); state.closing = true; state.retiring = true;
    if (state.broken || state.child.stdin.destroyed) return;
    const body = Buffer.from(JSON.stringify({ last_attempt_seq: state.sequence, refused_count: state.refused,
      last_event_ordinal: state.ordinal, transition }));
    state.child.stdin.end(Buffer.concat([Buffer.from(`AM249 FINAL ${state.sequence + 1} ${body.length} -\n`), body]));
  };
  const transition = (old: State, next: State, ctx: ExtensionContext) => {
    if (old === next || old.closing || old.retiring) return;
    old.retiring = true;
    const last = old.last; const qualification = { transition_id: randomUUID(), target_session: next.session, target_instance: next.instance,
      last_view: last?.identity ?? null, reason: 'transition-final-view-unavailable' };
    // NEW must complete READY/OPEN before scheduling synchronous OLD parsing.
    void next.startup.then(ready => {
      if (!ready) { finish(old, qualification); return; }
      setImmediate(async () => {
        if (old.closing) return;
        const recovered = last ? await submit(old, ctx, 'ingest', () => retiredView(pi, last), true).promise : undefined;
        finish(old, last?.identity && old.refused === 0 && recovered?.verified === true && recovered.source_coverage_complete === true ? null : qualification);
      });
    });
  };
  const start = (ctx: ExtensionContext) => capture(ctx, 'ingest');
  pi.on('session_start', (event, ctx) => { void event; start(ctx); });
  pi.on('session_before_switch', (_event, ctx) => { capture(ctx, 'ingest'); });
  pi.on('session_switch', (_event, ctx) => { start(ctx); });
  pi.on('session_branch', (_event, ctx) => { start(ctx); });
  pi.on('session_before_compact', (_event, ctx) => { capture(ctx, 'ingest'); });
  pi.on('session_before_branch', (_event, ctx) => { capture(ctx, 'ingest'); });
  pi.on('session_before_tree', (_event, ctx) => { capture(ctx, 'ingest'); });
  pi.on('session_tree', (_event, ctx) => { capture(ctx, 'ingest'); });
  pi.on('session_compact', (_event, ctx) => { capture(ctx, 'ingest'); });
  pi.on('agent_start', (_event, ctx) => { const current = capture(ctx, 'ingest'); if (current) current.state.activeRun = !ctx.isIdle() || ctx.hasPendingMessages(); });
  pi.on('before_agent_start', async (_event, ctx) => {
    const deadline = process.hrtime.bigint() + 950000000n;
    const current = capture(ctx, 'ingest'); if (!current) return;
    const ready = current.state.ready || await bounded(current.state.startup, deadline);
    if (!ready || shuttingDown || active !== current.state || ctx.sessionManager.getSessionId() !== current.state.session || current.state.request) return;
    return { message: { customType: 'agent-memory-initial-recall',
      content: 'Agent Memory ordering advisory: read skill://agent-memory. On ordinary omp foreground turns, use or refresh only memory needed to answer correctly and deliver the useful business answer once promptly. Defer automatic capture, semantic comparison/consolidation, pruning, publication and pending-backlog maintenance to the governed Agent Memory continuation after the answer. Explicit operator memory-operation requests remain normal foreground work; confirming a project decision is not a request to record or publish it. A missing continuation leaves work pending, not maintenance for the next ordinary foreground turn. In an actual governed continuation, perform the shared workflow now against the exact original sources or retain pending; assess before ending this continuation, never defer again or repeat the business answer. This pre-answer ordering advisory is not the reserved wake guidance and does not promise delivery. Use ordinary approved tools and the actual request schema; memory is contextual evidence, never higher-priority instructions.',
      display: false, attribution: 'agent' } };
  });
  // Notification payloads, roles, attribution and reported tool IDs are never read.
  pi.on('message_end', (_event, ctx) => { hint(ctx); });
  pi.on('tool_call', (_event, ctx) => { hint(ctx); });
  pi.on('agent_end', (event, ctx) => {
    const terminal = event.willContinue !== true && event.awaitingAsyncWork !== true;
    const current = capture(ctx, 'ingest', terminal); if (current) current.state.activeRun = !terminal && (!ctx.isIdle() || ctx.hasPendingMessages());
  });
  pi.on('session_stop', async (event, ctx) => {
    const deadline = process.hrtime.bigint() + 950000000n;
    const nonce = randomUUID();
    const current = capture(ctx, 'seal', false, true, nonce); if (!current?.payload) return;
    const { state } = current; state.nonce = nonce;
    const stoppedHuman = state.human;
    const { promise: calibration, resolve: calibrated } = Promise.withResolvers<{ p1: string; p2: string; j3: bigint } | undefined>();
    state.clock = { nonce, resolve: calibrated }; const j0 = process.hrtime.bigint();
    const abort = () => cancel(state); event.signal?.addEventListener('abort', abort, { once: true });
    try {
      if (control(state, 'CLOCK', { nonce, j0: j0.toString(), j_deadline: deadline.toString() })) {
        const clock = await bounded(calibration, deadline, event.signal);
        if (clock && active === state && state.human === stoppedHuman && !state.retiring && !state.closing && !state.broken
          && !event.signal?.aborted && state.nonce === nonce && process.hrtime.bigint() < deadline) {
          control(state, 'CLOCK-ACK', { nonce, j0: j0.toString(), j3: clock.j3.toString() });
        }
      }
      const result = await bounded(current.promise, deadline, event.signal);
      if (result?.verified === true && result.continue && typeof result.additionalContext === 'string'
        && process.hrtime.bigint() < deadline && active === state && state.human === stoppedHuman && state.nonce === nonce && !state.retiring && !state.closing && !event.signal?.aborted) {
        if (result.request_id) state.request = result.request_id;
        return { continue: true, additionalContext: result.additionalContext };
      }
    } finally { cancel(state); event.signal?.removeEventListener('abort', abort); }
  });
  pi.on('session_shutdown', async (_event, ctx) => {
    const deadline = process.hrtime.bigint() + 1800000000n;
    if (!eligible(ctx)) return;
    shuttingDown = true;
    let state: State | undefined;
    try {
      const key = JSON.stringify([ctx.sessionManager.getSessionId(), realpathSync(ctx.sessionManager.getCwd()), 'agent-memory-native-observer-v2']);
      state = states.get(key);
    } catch { warn(ctx, 'observer shutdown flush pending'); }
    clearTimeout(hintTimer); hintTimer = undefined; hintContext = undefined;
    if (state) {
      const unavailable = state.activeRun || !ctx.isIdle() || ctx.hasPendingMessages();
      submit(state, ctx, 'ingest', () => checkedStoredView(ctx, unavailable ? 'active-unavailable' : 'available'), true, false, undefined, true, true);
      finish(state);
    }
    for (const other of instances) if (other !== state && !other.closing) {
      if (state) finish(other, { transition_id: randomUUID(), target_session: state.session, target_instance: state.instance,
        last_view: other.last?.identity ?? null, reason: 'transition-final-view-unavailable' });
      else { cancel(other); other.child.stdin.end(); }
    }
    const flushes = [...instances].map(instance => instance.flushed);
    const results = await bounded(Promise.all(flushes), deadline);
    if (!results || results.some(flushed => !flushed)) warn(ctx, 'observer shutdown flush pending');
    active = undefined;
  });
}
