// Một bundle cho mọi cửa sổ của app: `?view=office` là văn phòng 2D (T13), mặc định là popup Lite (T12).
import { LitePopup } from './views/LitePopup';

export function App() {
  const view = new URLSearchParams(window.location.search).get('view');
  if (view === 'office') {
    return (
      <main className="popup">
        <h1>Văn phòng</h1>
        <p className="muted">Văn phòng 2D sẽ có ở T13.</p>
      </main>
    );
  }
  return <LitePopup />;
}
