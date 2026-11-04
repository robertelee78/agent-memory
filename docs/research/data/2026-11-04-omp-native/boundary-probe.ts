// Disposable native-interface research fixture. Not an agent-memory adapter.
import { appendFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { ExtensionAPI, ExtensionContext } from '@oh-my-pi/pi-coding-agent';
export default function (pi: ExtensionAPI) {
  const root = process.env.OMP_PROBE_ROOT!;
  const scenario = process.env.OMP_PROBE_SCENARIO || 'allowed';
  let requested = false;
  const record = (kind: string, value: unknown) => appendFileSync(join(root, 'observations.jsonl'), JSON.stringify({kind, scenario, at:Date.now(), value}) + '\n', {mode:0o600});
  const snapshot = (ctx: ExtensionContext) => structuredClone({sessionId:ctx.sessionManager.getSessionId(), leafId:ctx.sessionManager.getLeafId(), branch:ctx.sessionManager.getBranch()});
  pi.on('session_start', (_e, ctx) => record('session_start', snapshot(ctx)));
  pi.on('before_agent_start', (e, ctx) => record('before_agent_start', {reason:e.reason,prompt:e.prompt,snapshot:snapshot(ctx)}));
  pi.on('message_end', (e, ctx) => record('message_end', {event:e,snapshot:snapshot(ctx)}));
  pi.on('tool_call', e => record('tool_call', e));
  pi.on('tool_result', e => record('tool_result', e));
  pi.on('agent_end', (e, ctx) => record('agent_end', {event:e,snapshot:snapshot(ctx)}));
  pi.on('session_stop', (e, ctx) => {
    record('session_stop', {event:{...e,signal:undefined},snapshot:snapshot(ctx)});
    if (scenario==='cap') return {continue:true, additionalContext:'Advisory cap research probe. Reply exactly CAP_TICK_249. Do not call tools or repeat the original substantive answer.'};
    if (requested) return;
    requested = true;
    record('continuation_requested', {sessionId:ctx.sessionManager.getSessionId()});
    return {continue:true, additionalContext:'Native interface probe only. Your substantive task answer is already delivered; do not repeat it. Call probe_boundary_action exactly once with {"label":"SOURCE_249"}. If refused, unavailable or cancelled, do not use any alternative tool or execution path. Then give only a brief boundary status. This is agent-attributed integration context, not a new human instruction.'};
  });
  pi.registerTool({
    name:'probe_boundary_action',label:'Boundary interface probe',description:'Disposable native permission/cancellation probe. Creates only a sentinel in the isolated probe root after approval. No memory-bank access.',
    parameters:pi.zod.object({label:pi.zod.string()}),approval:'exec',loadMode:'essential',
    async execute(id, params, signal) {
      record('tool_execute_started',{id,params});
      if (scenario==='cancel' || scenario==='steer') {
        const {promise,resolve,reject}=Promise.withResolvers<void>();
        const timer=setTimeout(resolve,scenario==='cancel'?15000:2500);
        const abort=()=>{clearTimeout(timer);record('tool_aborted',{id});reject(new Error('probe cancelled before sentinel'));};
        if(signal?.aborted) abort(); else signal?.addEventListener('abort',abort,{once:true});
        await promise;
      }
      if (scenario==='offline') {record('tool_offline',{id});throw new Error('probe authority unavailable; original source remains pending');}
      writeFileSync(join(root,'approved-sentinel.json'),JSON.stringify({id,params}),{mode:0o600});
      record('tool_execute_completed',{id,params});
      return {content:[{type:'text',text:'PROBE_BOUNDARY_ACTION_COMPLETED SOURCE_249'}],details:{probeOnly:true}};
    }
  });
}
