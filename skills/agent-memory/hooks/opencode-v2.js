// The shared prompt invokes the portable skill; it is not a capture receipt.
import { readFileSync, realpathSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

let prompt
try {
  const directory = dirname(realpathSync(fileURLToPath(import.meta.url)))
  prompt = readFileSync(join(directory, 'boundary-prompt.txt'), 'utf8').trim()
  if (!prompt || Buffer.byteLength(prompt, 'utf8') > 4096) throw new Error('invalid prompt')
} catch {
  prompt = 'Agent Memory turn boundary: hook instruction unavailable; memory refresh and capture are pending.'
}
const marker = 'Agent Memory turn boundary:'

export default {
  id: 'agent-memory-turn-boundary',
  setup: async (ctx) => {
    const registration = await ctx.session.hook('context', (event) => {
      try {
        if (!Array.isArray(event?.system)) return
        if (event.system.some((part) => typeof part?.text === 'string' && part.text.startsWith(marker))) return
        event.system.push({ type: 'text', text: prompt })
      } catch { /* Preserve the user's turn; no capture or freshness is claimed. */ }
    })
    return () => registration.dispose()
  },
}
