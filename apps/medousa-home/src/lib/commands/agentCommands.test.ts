/** @vitest-environment happy-dom */
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkshopCommandContext } from "./types";
const state = vi.hoisted(() => ({scope:"mac",bots:[] as any[],agents:[] as any[]}));
const api = vi.hoisted(() => ({open:vi.fn(),tab:vi.fn(),switch:vi.fn(),close:vi.fn(),focus:vi.fn()}));
vi.mock("$lib/stores/chat.svelte", () => ({chat:{get workshopScopeId(){return state.scope;},sessions:[]}}));
vi.mock("$lib/stores/bots.svelte", () => ({bots:{get bots(){return state.bots;},forSession:()=>null,open:api.open}}));
vi.mock("$lib/stores/connectedAgents.svelte", () => ({connectedAgents:{workshopScopeId:"mac",get conversations(){return state.agents;}}}));
vi.mock("$lib/stores/shellTabs.svelte", () => ({shellTabs:{openChat:api.tab}}));
import { buildBotOpenCommands, buildConnectedAgentOpenCommands } from "./searchProviders";
import { buildDoCommands } from "./doCommands";
import { agentCreation } from "$lib/stores/agentCreation.svelte";
const ctx = {chat:{get workshopScopeId(){return state.scope;},switchSession:api.switch},callbacks:{close:api.close,focusChat:api.focus}} as unknown as WorkshopCommandContext;
beforeEach(()=>{vi.clearAllMocks();state.scope="mac";state.bots=[{bot_id:"ada",display_name:"Ada",role_description:"Organize groceries",primary_manuscript_id:"organizer",avatar_ref:"purple",archived:false},{bot_id:"archived",display_name:"Old",archived:true}];state.agents=[{id:"prox",provider:"grok_bot",label:"Prox"}];agentCreation.close();});
describe("Spotlight agent identities",()=>{
  it("finds a Bot by purpose and opens the binding returned by the workshop",async()=>{
    api.open.mockResolvedValue({bot:{display_name:"Ada"},binding:{session_id:"primary"}});
    const hits=buildBotOpenCommands("groceries");expect(hits).toHaveLength(1);await hits[0].run(ctx);
    expect(api.switch).toHaveBeenCalledWith("primary");expect(api.tab).toHaveBeenCalledWith("primary",{title:"Ada",activate:true});expect(buildBotOpenCommands("Old")).toHaveLength(0);
  });
  it("opens a connected agent’s stable identity and scopes its search",async()=>{
    const hits=buildConnectedAgentOpenCommands("Grok");expect(hits).toHaveLength(1);await hits[0].run(ctx);
    expect(api.switch).toHaveBeenCalledWith("external-conversation:grok_bot:prox");state.scope="mini";expect(buildConnectedAgentOpenCommands("")).toHaveLength(0);
  });
  it("launches shared creation dialogs through Spotlight create commands",async()=>{
    await buildDoCommands().find((item)=>item.id==="do-create-bot")!.run(ctx);expect(agentCreation.kind).toBe("bot");
    await buildDoCommands().find((item)=>item.id==="do-connect-agent")!.run(ctx);expect(agentCreation.kind).toBe("connection");expect(api.close).toHaveBeenCalledTimes(2);
  });
});
