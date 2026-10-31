import { agentIcon } from './agent-icons';
import {
  allInstalled,
  integrationAgents,
  type AgentIntegration,
  type IntegrationInfo,
  type IntegrationResource,
} from './integration';

export interface AgentStripLabels {
  all: string;
  empty: string;
  installFor: string;
  removeFrom: string;
}

const STACK_SHIFT = 8;

/**
 * Draw Magpie-style Agent chips for one resource.
 */
export function renderAgentStrip(
  root: HTMLElement,
  info: IntegrationInfo,
  resource: IntegrationResource,
  labels: AgentStripLabels,
  busy: boolean,
  onChange: (agentIds: string[], enabled: boolean) => void,
) {
  const agents = integrationAgents(info, resource);
  root.replaceChildren();
  if (agents.length === 0) {
    const empty = document.createElement('span');
    empty.className = 'agent-empty';
    empty.textContent = labels.empty;
    root.append(empty);
    return;
  }

  const eligible = agents.filter((agent) => !agent[resource]?.conflict);
  const everyOn = allInstalled(info, resource);
  const all = document.createElement('button');
  all.type = 'button';
  all.className = `agent-chip agent-chip-all${everyOn ? ' is-on' : ''}`;
  all.textContent = labels.all;
  all.setAttribute('aria-pressed', String(everyOn));
  all.style.setProperty('--agent-stack-shift', `${(agents.length - 1) * STACK_SHIFT}px`);
  all.disabled = busy || eligible.length === 0;
  all.addEventListener('click', () => {
    onChange(
      eligible.map((agent) => agent.id),
      !everyOn,
    );
  });

  const stack = document.createElement('div');
  stack.className = 'agent-icon-stack';
  for (const [index, agent] of agents.entries()) {
    const button = agentButton(agent, resource, labels, busy, onChange);
    button.style.setProperty(
      '--agent-stack-shift',
      `${(agents.length - index - 1) * STACK_SHIFT}px`,
    );
    stack.append(button);
  }
  root.append(all, stack);
}

function agentButton(
  agent: AgentIntegration,
  resource: IntegrationResource,
  labels: AgentStripLabels,
  busy: boolean,
  onChange: (agentIds: string[], enabled: boolean) => void,
) {
  const state = agent[resource]!;
  const button = document.createElement('button');
  button.type = 'button';
  button.className = [
    'agent-chip',
    state.installed ? 'is-on' : '',
    state.conflict ? 'has-conflict' : '',
  ]
    .filter(Boolean)
    .join(' ');
  button.dataset.agent = agent.id;
  button.setAttribute('aria-pressed', String(state.installed));
  const action = state.installed
    ? `${labels.removeFrom} ${agent.name}`
    : labels.installFor.replace('{agent}', agent.name);
  button.setAttribute('aria-label', action);
  button.title = state.conflict
    ? `${agent.name}: ${state.conflict}`
    : state.installed
      ? `${action}\n${state.path}`
      : `${action}${state.path}`;
  button.disabled = busy;

  const icon = agentIcon(agent.icon);
  if (icon?.monochrome) {
    const mask = document.createElement('span');
    mask.className = 'agent-icon-mask';
    mask.setAttribute('aria-hidden', 'true');
    mask.style.setProperty('--agent-icon-url', `url("${icon.url}")`);
    button.append(mask);
  } else if (icon) {
    const image = document.createElement('img');
    image.src = icon.url;
    image.alt = '';
    image.draggable = false;
    button.append(image);
  } else {
    const fallback = document.createElement('span');
    fallback.className = 'agent-chip-fallback';
    fallback.textContent = agent.name.slice(0, 1).toUpperCase();
    button.append(fallback);
  }
  button.addEventListener('click', () => onChange([agent.id], !state.installed));
  return button;
}
