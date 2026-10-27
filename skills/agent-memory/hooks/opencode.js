// One discovery path supports co-installed OpenCode 1.x and 2.x. The actual
// hosting executable, not PATH, selects the documented plugin API shape.
import { spawnSync } from 'node:child_process'
import { AgentMemoryBoundary } from './opencode-v1.js'
import v2 from './opencode-v2.js'

let major = null
try {
  const result = spawnSync(process.execPath, ['--version'], { encoding: 'utf8', timeout: 3000 })
  const match = result.status === 0 && result.stdout.match(/(?:^|\s|v)([12])\.\d+\.\d+/)
  if (match) major = Number(match[1])
} catch { /* The v2 shape is conservative for unknown/future clients. */ }

export default major === 1 ? AgentMemoryBoundary : v2
