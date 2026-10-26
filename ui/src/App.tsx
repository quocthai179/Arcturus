// Một bundle cho mọi cửa sổ: Office (T13) và popup Lite (T12).
import { Office } from './views/Office';
import { LitePopup } from './views/LitePopup';

export function App() {
  const view = new URLSearchParams(window.location.search).get('view');
  if (view === 'office') {
    return <Office />;
  }
  return <LitePopup />;
}
