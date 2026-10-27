// The shared prompt invokes the portable skill; it is not a capture receipt.
import { readFileSync, realpathSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

let prompt
try {
  const directory = dirname(realpathSync(fileURLToPath(import.meta.url)))
  prompt = readFileSync(join(directory, 'boundary-prompt.txt'), 'utf8').trim().replaceAll('{{SKILL_DIR}}', dirname(directory))
  if (!prompt || Buffer.byteLength(prompt, 'utf8') > 4096) throw new Error('invalid prompt')
} catch {
  prompt = 'Agent Memory turn boundary: hook instruction unavailable; memory refresh and capture are pending.'
}
const marker = 'Agent Memory turn boundary:'
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
      const registration = await ctx.session.hook('context', (event) => {
        try {
          if (!Array.isArray(event?.system)) { pending(); return }
          if (event.system.some((part) => typeof part?.text === 'string' && part.text.startsWith(marker))) return
          event.system.push({ type: 'text', text: prompt })
        } catch { pending() }
      })
      if (typeof registration?.dispose !== 'function') { pending(); return () => {} }
      return () => {
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
