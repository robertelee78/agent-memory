// Agent Memory for omp: at session start, ask `am` for the project memory
// summary and hand it to the first prompt of the session. Nothing else.
import { spawn } from 'node:child_process';
import type { ExtensionAPI } from '@oh-my-pi/pi-coding-agent';

type Options = { executable: string };

function summary(executable: string, cwd: string): Promise<string> {
  const { promise, resolve } = Promise.withResolvers<string>();
  let out = '';
  try {
    const child = spawn(executable, ['hook', 'session-start', '--client', 'omp'], { cwd, stdio: ['ignore', 'pipe', 'ignore'] });
    const timer = setTimeout(() => child.kill(), 2000);
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', chunk => { out += chunk; });
    child.on('error', () => { clearTimeout(timer); resolve(''); });
    child.on('close', () => { clearTimeout(timer); resolve(out.trim()); });
  } catch { resolve(''); }
  return promise;
}

export default function agentMemory(pi: ExtensionAPI, options: Options) {
  const pending = new Map<string, Promise<string>>();
  pi.on('session_start', (_event, ctx) => {
    pending.set(ctx.sessionManager.getSessionId(), summary(options.executable, ctx.sessionManager.getCwd()));
  });
  pi.on('before_agent_start', async (_event, ctx) => {
    const key = ctx.sessionManager.getSessionId();
    const text = pending.get(key);
    if (!text) return;
    pending.delete(key);
    const content = await text;
    if (!content) return;
    return { message: { customType: 'agent-memory-session-start', content, display: false, attribution: 'agent' } };
  });
}
