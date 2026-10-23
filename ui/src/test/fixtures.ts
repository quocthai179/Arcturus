import type { AgentView, ApprovalView, HandoffView, ServerMsg } from '../protocol';

export function agent(over: Partial<AgentView> = {}): AgentView {
  return {
    id: 'agt_s1',
    parent_id: null,
    kind: 'session',
    name: 'Khoa',
    role: 'backend',
    palette: { hair: '#1F1F24', outfit: '#4C8DF6', acc: '#9AA3B5' },
    mood: 'approval',
    activity: 'waiting',
    act_text: 'Chờ duyệt: Bash',
    host: 'laptop',
    source: 'claude-code',
    cwd: '/w/proj',
    context_pct: null,
    last_event_at: '2026-10-23T09:00:00Z',
    ...over,
  };
}

export function approval(over: Partial<ApprovalView> = {}): ApprovalView {
  const command = over.command_normalized ?? 'git push origin main';
  return {
    request_id: 'req_1',
    agent_id: 'agt_s1',
    agent_name: 'Khoa',
    tool: 'Bash',
    command_original: command,
    tool_options: {},
    command_normalized: command,
    danger_spans: [],
    suspicions: [],
    truncated: false,
    risk: 'medium',
    reversible: false,
    group_key: 'Bash',
    created_at: '2026-10-23T09:00:00Z',
    deadline_at: '2099-01-01T00:00:00Z',
    countdown_ms: null,
    ...over,
  };
}

export function handoff(over: Partial<HandoffView> = {}): HandoffView {
  return {
    handoff_id: 'hof_1',
    agent_id: 'agt_s1',
    agent_name: 'Khoa',
    tool: 'Bash',
    bytes: 18230,
    lines: 412,
    preview_head: '#!/bin/sh\nset -e',
    preview_tail: 'echo done',
    suspicions: [],
    tool_options: {},
    message: 'Lệnh quá dài để duyệt ở đây — hãy mở terminal của Khoa để xem đủ và quyết định.',
    created_at: '2026-10-23T09:00:00Z',
    ...over,
  };
}

export function snapshot(over: Partial<Extract<ServerMsg, { t: 'snapshot' }>> = {}): ServerMsg {
  return {
    t: 'snapshot',
    agents: [agent()],
    queue: [],
    handoffs: [],
    team: { paused: false, focus: false, roster_max: 5 },
    ...over,
  };
}
