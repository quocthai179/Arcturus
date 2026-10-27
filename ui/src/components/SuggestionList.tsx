// Đề xuất nâng mức tin cậy (docs/PLAN.md T10). Chữ do hub dựng từ template (không LLM); mọi chuỗi vẫn là dữ liệu không tin cậy
// nên chỉ render như văn bản thuần. Đồng ý chỉ cho phép tự chạy yêu cầu rủi ro thấp/vừa của đúng nhóm lệnh đó.
import type { TrustSuggestionView } from '../protocol';

export interface SuggestionListProps {
  suggestions: readonly TrustSuggestionView[];
  onAnswer: (id: string, accept: boolean) => void;
}

export function SuggestionList({ suggestions, onAnswer }: SuggestionListProps) {
  if (suggestions.length === 0) return null;
  return (
    <section aria-label="Đề xuất">
      {suggestions.map((s) => (
        <article key={s.id} className="suggestion" aria-label={`Đề xuất cho ${s.subject}`}>
          <p>{s.text}</p>
          <div className="suggestion__actions">
            <button type="button" className="btn btn--allow" onClick={() => onAnswer(s.id, true)}>
              Cho phép
            </button>
            <button type="button" className="btn btn--deny" onClick={() => onAnswer(s.id, false)}>
              Để sau
            </button>
          </div>
        </article>
      ))}
    </section>
  );
}
