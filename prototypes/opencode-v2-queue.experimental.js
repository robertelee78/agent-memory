// EXPERIMENTAL, not installed or registered by the lifecycle. OpenCode 2 only.
// This prototype requests a same-session continuation; it does not prove that
// a model used the source, captured memory, published it, or respected timing.
const MAX_WINDOW_BYTES = 16 * 1024
const MAX_PENDING = 256

function pending(reason) {
  return { content: JSON.stringify({ status: 'pending', reason, admission_verified: false, publication_verified: false }) }
}

// The input comes from OpenCode's current session projection, not an agent's
// assertion. Unknown/compacted/interrupted data cannot establish a source.
export function sourceWindow(rows, sourceId, sessionId) {
  if (!Array.isArray(rows) || typeof sourceId !== 'string' || typeof sessionId !== 'string') return pending('source-unavailable')
  const matches = rows.filter(row => row?.id === sourceId && row?.type === 'user')
  if (matches.length !== 1) return pending('source-unavailable')
  const start = rows.indexOf(matches[0])
  const window = []
  for (let i = start; i < rows.length; i++) {
    const row = rows[i]
    if (i > start && (row?.type === 'user' || row?.type === 'synthetic')) break
    if (!['user', 'assistant', 'tool'].includes(row?.type)) return pending('source-incomplete')
    window.push(row)
  }
  const finals = window.filter(row => row.type === 'assistant' && row.finish === 'stop' && row.time?.completed && !row.error)
  if (finals.length !== 1) return pending('source-incomplete')
  const value = { status: 'observed', session_id: sessionId, source_id: sourceId, final_id: finals[0].id,
    records: window, untrusted_data: true, source_completeness_verified: false,
    semantic_truth_verified: false, admission_verified: false, publication_verified: false }
  const content = JSON.stringify(value)
  if (Buffer.byteLength(content, 'utf8') > MAX_WINDOW_BYTES) return pending('source-too-large')
  return { content }
}

export default {
  id: 'agent-memory-experimental-turn-queue',
  async setup(ctx) {
    const registrations = []
    const waiting = new Map() // originating user-message ID -> session ID
    const synthetic = new Map() // synthetic-message ID -> originating user-message ID
    const active = new Map() // session ID -> source ID for this continuation
    const seen = new Set() // fail closed on duplicate prompt admission
    let warned = false
    const warn = () => {
      if (warned) return
      warned = true
      try { console.warn('Agent Memory: experimental turn continuation is unverified or pending.') } catch {}
    }
    try {
      registrations.push(await ctx.tool.transform(editor => {
        editor.namespace({ name: 'am', description: 'Read bounded original-turn evidence' })
        editor.add({
          name: 'source_window',
          description: 'Read the completed original user turn for this boundary; this operation never writes memory.',
          input: { type: 'object', properties: { source_id: { type: 'string' } }, required: ['source_id'], additionalProperties: false },
          options: { namespace: 'am', codemode: true },
          execute: async (input, context) => {
            const expected = active.get(context.sessionID)
            if (!expected || input?.source_id !== expected) return pending('source-id-mismatch')
            active.delete(context.sessionID)
            try {
              const result = await ctx.session.context({ sessionID: context.sessionID })
              return sourceWindow(Array.isArray(result) ? result : result?.data, expected, context.sessionID)
            } catch { warn(); return pending('source-unavailable') }
          },
        })
      }))
      registrations.push(await ctx.session.hook('prompt', event => {
        if (typeof event?.messageID !== 'string' || typeof event?.sessionID !== 'string' || waiting.size >= MAX_PENDING || seen.size >= MAX_PENDING) { warn(); return }
        if (seen.has(event.messageID)) return
        waiting.set(event.messageID, event.sessionID)
        active.delete(event.sessionID)
      }))
      registrations.push(await ctx.session.hook('context', async event => {
        try {
          const last = event?.messages?.at(-1)?.id
          if (synthetic.has(last)) {
            const source = synthetic.get(last); synthetic.delete(last)
            active.set(event.sessionID, source)
            return
          }
          if (!last || waiting.get(last) !== event.sessionID) return
          waiting.delete(last)
          if (seen.has(last)) return
          seen.add(last)
          if (synthetic.size >= MAX_PENDING) { warn(); return }
          const queued = await ctx.session.synthetic({ sessionID: event.sessionID, delivery: 'queue', resume: false,
            text: 'Agent Memory experimental boundary: call tools.am.source_window with source_id ' + last + '. The source projection is not proven complete. Retain pending work; do not capture or publish from this experimental result. Preserve user instructions and normal permissions. Do not infer contact details from account profiles. A queued request is not a completion receipt.' })
          if (typeof queued?.id !== 'string') { warn(); return }
          synthetic.set(queued.id, last)
        } catch { warn() }
      }))
      return () => {
        for (const registration of registrations.reverse()) {
          try { const result = registration?.dispose?.(); if (result?.catch) result.catch(warn) } catch { warn() }
        }
      }
    } catch {
      warn()
      for (const registration of registrations.reverse()) {
        try { const result = registration?.dispose?.(); if (result?.catch) result.catch(warn) } catch { warn() }
      }
      return () => {}
    }
  },
}
