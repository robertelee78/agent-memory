// The shared prompt invokes the portable skill; it is not a capture receipt.
import { readFileSync, realpathSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

let prompt
try {
  const directory = dirname(realpathSync(fileURLToPath(import.meta.url)))
  prompt = readFileSync(join(directory, 'boundary-prompt.txt'), 'utf8').trim().replaceAll('{{SKILL_DIR}}', () => dirname(directory))
  if (!prompt || Buffer.byteLength(prompt, 'utf8') > 4096) throw new Error('invalid prompt')
} catch {
  prompt = 'Agent Memory turn boundary: hook instruction unavailable; memory refresh and capture are pending.'
}
const marker = 'Agent Memory turn boundary:'

export const AgentMemoryBoundary = async () => ({
  'experimental.chat.system.transform': async (_input, output) => {
    try {
      if (!Array.isArray(output?.system)) return
      if (!output.system.some((part) => typeof part === 'string' && part.startsWith(marker))) output.system.push(prompt)
    } catch { /* Preserve the user's turn; no capture or freshness is claimed. */ }
  },
})
