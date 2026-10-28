import queue from './opencode-v2-queue.js'
// The shared prompt invokes the portable skill; it is not a capture receipt.
import { readFileSync, realpathSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

let prompt
try {
  const directory = dirname(realpathSync(fileURLToPath(import.meta.url)))
  prompt = readFileSync(join(directory, 'boundary-prompt.txt'), 'utf8').trim().replaceAll('{{SKILL_DIR}}', () => dirname(directory))
  if (!prompt || Buffer.byteLength(prompt, 'utf8') > 2048) throw new Error('invalid prompt')
} catch {
  prompt = 'Agent Memory turn boundary: preserve the substantive task answer. Follow the installed agent-memory turn operation for actual source assessment; routine no-learning bookkeeping stays quiet. If tools or instructions are unavailable, retain pending and add only a concise consequential qualification. Never infer capture or publication from hook execution.'
}
const marker = 'Agent Memory turn boundary:'
const identityMarker = 'Agent Memory observed OpenCode session:'
const conflictMarker = 'Agent Memory session identity conflict:'
const foregroundMarker = 'Agent Memory OpenCode foreground:'
let warned = false
function pending () {
  if (warned) return
  warned = true
  try { console.warn('Agent Memory: OpenCode turn hook unavailable; automatic refresh and capture remain pending.') } catch {}
}

export default {
  id: 'agent-memory-turn-boundary',
  setup: async (ctx) => {
    try {
      let disposeQueue = () => {}
      const registration = await ctx.session.hook('context', (event) => {
        try {
          if (!Array.isArray(event?.system)) { pending(); return }
          // The harness provides the session ID directly. Expose this as
          // provenance metadata, without requesting any foreground work.
          const prior = event.system.filter((part) => typeof part?.text === 'string' && part.text.startsWith(identityMarker))
          if (typeof event?.sessionID === 'string' && /^ses_[A-Za-z0-9_-]{1,180}$/.test(event.sessionID)) {
            const identity = `${identityMarker} ${event.sessionID}. If recording session provenance, use this observed identifier; never invent one from a workspace path. This metadata does not request foreground memory work.`
            if (prior.length === 0) {
              event.system.push({ type: 'text', text: identity })
            } else if (prior.length !== 1 || prior[0].text !== identity) {
              // Keep other providers' context intact and make disagreement explicit.
              if (!event.system.some((part) => typeof part?.text === 'string' && part.text.startsWith(conflictMarker))) {
                event.system.push({ type: 'text', text: `${conflictMarker} The current native OpenCode session is ${event.sessionID}, but conflicting session metadata is present. Preserve pending work; do not record or acknowledge provenance until the conflict is resolved.` })
              }
              pending()
            }
          } else if (prior.length || disposeQueue.active || event?.sessionID !== undefined) {
            if (!event.system.some((part) => typeof part?.text === 'string' && part.text.startsWith(conflictMarker))) {
              event.system.push({ type: 'text', text: `${conflictMarker} The native OpenCode session ID is unavailable or invalid. Preserve pending work; do not record or acknowledge session provenance.` })
            }
            pending()
          }
          if (disposeQueue.active) {
            if (!event.system.some((part) => typeof part?.text === 'string' && (part.text.startsWith(foregroundMarker) || part.text.startsWith(conflictMarker)))) {
              event.system.push({ type: 'text', text: `${foregroundMarker} For an ordinary user turn, use and refresh relevant memory as needed to answer the user's request, and deliver the response promptly. Defer automatic capture, comparison and publication to the queued Agent Memory continuation; do not repeat those steps in the foreground. If the user explicitly asks for a memory operation, perform that requested operation normally. In the queued continuation, perform the governed boundary workflow rather than deferring it again. Preserve pending outcomes and normal permissions.` })
            }
            return
          }
          if (event.system.some((part) => typeof part?.text === 'string' && part.text.startsWith(marker))) return
          event.system.push({ type: 'text', text: prompt })
        } catch { pending() }
      })
      if (typeof registration?.dispose !== 'function') { pending(); return () => {} }
      // Register foreground shaping first, so exact-control transport is last.
      try { disposeQueue = await queue.setup(ctx) } catch { pending() }
      return () => {
        try { disposeQueue?.() } catch { pending() }
        try {
          const result = registration.dispose()
          if (result && typeof result.catch === 'function') return result.catch(pending)
          return result
        } catch { pending() }
      }
    } catch {
      pending()
      return () => {}
    }
  },
}
