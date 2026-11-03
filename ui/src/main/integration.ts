import type { AgentIntegration, IntegrationResource, IntegrationState } from '../types/rust';

export type {
  AgentResourceState,
  AgentIntegration,
  AgentProblem,
  IntegrationMutation,
  IntegrationResource,
  IntegrationState as IntegrationInfo,
} from '../types/rust';

type IntegrationInfo = IntegrationState;

/**
 * Render the MCP display command with the default home omitted by the backend.
 */
export function mcpCommandLine(info: Pick<IntegrationInfo, 'mcpCommand' | 'mcpDisplayArgs'>) {
  return [info.mcpCommand, ...info.mcpDisplayArgs]
    .map((part) => (/[\s"']/u.test(part) ? JSON.stringify(part) : part))
    .join(' ');
}

/** Agents that can receive one resource, as the page's icon row shows them. */
export function integrationAgents(
  info: IntegrationInfo,
  resource: IntegrationResource,
): AgentIntegration[] {
  return info.agents.filter((agent) => agent[resource] !== undefined);
}

/** Whether every Agent shown on that row has the resource installed. */
export function allInstalled(info: IntegrationInfo, resource: IntegrationResource): boolean {
  const agents = integrationAgents(info, resource).filter((agent) => !agent[resource]?.conflict);
  return agents.length > 0 && agents.every((agent) => agent[resource]?.installed);
}
