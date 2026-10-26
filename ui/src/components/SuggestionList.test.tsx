import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { TrustSuggestionView } from '../protocol';
import { SuggestionList } from './SuggestionList';

afterEach(cleanup);

export function suggestion(over: Partial<TrustSuggestionView> = {}): TrustSuggestionView {
  return {
    id: 'sug_1',
    subject: 'Khoa',
    pattern: 'Bash:npm test',
    level: 6,
    approved_in_row: 20,
    window_days: 7,
    grant_days: 30,
    text: 'Khoa đã được bạn duyệt 20 lần liên tiếp cho "Bash:npm test" trong 7 ngày. Cho phép tự chạy trong 30 ngày?',
    created_at: '2026-10-27T10:00:00Z',
    ...over,
  };
}

describe('SuggestionList', () => {
  it('renders_nothing_without_suggestions', () => {
    const { container } = render(<SuggestionList suggestions={[]} onAnswer={() => {}} />);
    expect(container.firstChild).toBeNull();
  });

  it('shows_the_hub_text_and_answers_accept_or_decline', () => {
    const onAnswer = vi.fn();
    render(<SuggestionList suggestions={[suggestion(), suggestion({ id: 'sug_2', subject: 'Linh' })]} onAnswer={onAnswer} />);
    expect(screen.getByLabelText('Đề xuất cho Khoa').textContent).toContain('20 lần liên tiếp');
    const [allow] = screen.getAllByRole('button', { name: 'Cho phép' });
    const [, later] = screen.getAllByRole('button', { name: 'Để sau' });
    fireEvent.click(allow as HTMLElement);
    fireEvent.click(later as HTMLElement);
    expect(onAnswer.mock.calls).toEqual([
      ['sug_1', true],
      ['sug_2', false],
    ]);
  });

  it('renders_untrusted_text_as_plain_text_not_html', () => {
    render(<SuggestionList suggestions={[suggestion({ text: '<img src=x onerror=alert(1)> "ok"' })]} onAnswer={() => {}} />);
    expect(document.querySelector('img')).toBeNull();
    expect(screen.getByLabelText('Đề xuất cho Khoa').textContent).toContain('<img src=x onerror=alert(1)>');
  });
});
