// Một bundle cho mọi cửa sổ: Office (T13) và popup Lite (T12).
import { Office } from './views/Office';
import { useMotionPolicy } from './useMotionPolicy';
import { LitePopup } from './views/LitePopup';

export function App() {
  useMotionPolicy();
  const view = new URLSearchParams(window.location.search).get('view');
  if (view === 'office') {
    return <Office />;
  }
  return <LitePopup />;
}
