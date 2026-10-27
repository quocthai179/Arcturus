// Một bundle cho mọi cửa sổ: Office (T13), popup Lite (T12) và trợ lý pet (docs/PLAN.md §7; phần 3D nạp muộn).
import { Office } from './views/Office';
import { useMotionPolicy } from './useMotionPolicy';
import { LitePopup } from './views/LitePopup';
import { Pet } from './views/Pet';

export function App() {
  useMotionPolicy();
  const view = new URLSearchParams(window.location.search).get('view');
  if (view === 'office') {
    return <Office />;
  }
  if (view === 'pet') {
    return <Pet />;
  }
  return <LitePopup />;
}
