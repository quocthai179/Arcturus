// Lệnh cho cả đội (docs/PLAN.md T14): "Tạm dừng cả đội" → team_pause/team_resume, "Giờ tập trung" → focus_set.
import { useState } from 'react';
import type { ClientMsg, TeamView } from '../protocol';

export const PAUSE_TOOLTIP =
  'Hub chỉ tắt tự duyệt và đếm ngược; không dừng được Claude Code. Agent vẫn chạy, lệnh cần duyệt vẫn hỏi như thường.';
export const FOCUS_TOOLTIP =
  'Ẩn chấm báo và thông báo cho yêu cầu rủi ro thấp/vừa. Rủi ro cao và vùng khóa vẫn báo. Yêu cầu vẫn nằm trong hàng đợi.';

export interface TeamControlsProps {
  team: TeamView;
  connected: boolean;
  send: (msg: ClientMsg) => boolean;
}

export function TeamControls({ team, connected, send }: TeamControlsProps) {
  const [failed, setFailed] = useState(false);
  // Bỏ focus khỏi nút vừa bấm: nếu không, Enter (phím Đồng ý của popup) sẽ bấm lại nút này thay vì duyệt thẻ.
  const run = (msg: ClientMsg, button: HTMLButtonElement) => {
    setFailed(!send(msg));
    button.blur();
  };
  return (
    <div className="team-controls">
      <button
        type="button"
        className="btn btn--team"
        aria-pressed={team.paused}
        title={PAUSE_TOOLTIP}
        disabled={!connected}
        onClick={(e) => run(team.paused ? { t: 'team_resume' } : { t: 'team_pause' }, e.currentTarget)}
      >
        {team.paused ? 'Tiếp tục cả đội' : 'Tạm dừng cả đội'}
      </button>
      <button
        type="button"
        className="btn btn--team"
        aria-pressed={team.focus}
        title={FOCUS_TOOLTIP}
        disabled={!connected}
        onClick={(e) => run({ t: 'focus_set', on: !team.focus }, e.currentTarget)}
      >
        Giờ tập trung{team.focus ? ': bật' : ''}
      </button>
      {failed && (
        <p className="error small" role="alert">
          Chưa gửi được lệnh, hub chưa kết nối.
        </p>
      )}
    </div>
  );
}
