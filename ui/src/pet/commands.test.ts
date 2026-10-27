import { describe, expect, it } from 'vitest';
import { COMMANDS, helpText, normalizeCommand, parseCommand } from './commands';

describe('parseCommand', () => {
  it('nhận lệnh có dấu, không dấu, hoa thường và dấu câu', () => {
    for (const s of ['Tạm dừng đội', 'tam dung doi', '  TẠM   DỪNG ĐỘI! ', 'pause']) {
      const r = parseCommand(s);
      expect(r.ok && r.action.kind === 'send' && r.action.msg.t).toBe('team_pause');
    }
    const focus = parseCommand('bật giờ tập trung');
    expect(focus.ok && focus.action.kind === 'send' && focus.action.msg).toEqual({ t: 'focus_set', on: true });
    expect(parseCommand('tóm tắt hôm nay')).toEqual({ ok: true, action: { kind: 'standup' } });
    expect(parseCommand('ai đang rảnh')).toEqual({ ok: true, action: { kind: 'who_idle' } });
  });

  it('lệnh lạ bị từ chối, không đoán gần đúng', () => {
    for (const s of ['', 'duyệt hết đi', 'allow all', 'cho phép tất cả', 'tạm', 'rm -rf /', 'đồng ý']) {
      expect(parseCommand(s).ok).toBe(false);
    }
  });

  it('không lệnh nào gửi quyết định duyệt, duyệt gộp, batch hay trả lời đề xuất tin cậy', () => {
    const allowed = new Set(['team_pause', 'team_resume', 'focus_set']);
    for (const c of COMMANDS) {
      if (c.action.kind === 'send') expect(allowed.has(c.action.msg.t)).toBe(true);
      for (const a of [c.label, ...c.aliases]) {
        const r = parseCommand(a);
        expect(r.ok).toBe(true);
        if (r.ok) expect(r.action).toBe(c.action);
      }
    }
  });

  it('không có bí danh trùng nhau giữa hai lệnh', () => {
    const seen = new Map<string, string>();
    for (const c of COMMANDS) {
      for (const a of [normalizeCommand(c.label), ...c.aliases]) {
        expect(seen.get(a) ?? c.label, `"${a}"`).toBe(c.label);
        seen.set(a, c.label);
      }
    }
  });

  it('trợ giúp liệt kê mọi lệnh', () => {
    for (const c of COMMANDS) expect(helpText()).toContain(c.label);
  });
});
