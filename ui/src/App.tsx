// Một bundle cho mọi cửa sổ: Office (T13), popup Lite (T12), trợ lý pet (docs/PLAN.md §7; phần 3D nạp muộn) và cửa sổ
// cài đặt hook (setup.rs).
import { lazy, Suspense } from 'react';
import { Office } from './views/Office';
import { useMotionPolicy } from './useMotionPolicy';
import { LitePopup } from './views/LitePopup';
import { Pet } from './views/Pet';

// Cửa sổ cài đặt hiếm khi mở: nạp muộn để không làm nặng bundle của popup (ngân sách mở popup, T12/T16).
const Setup = lazy(() => import('./views/Setup').then((m) => ({ default: m.Setup })));

export function App() {
  useMotionPolicy();
  const view = new URLSearchParams(window.location.search).get('view');
  if (view === 'office') {
    return <Office />;
  }
  if (view === 'pet') {
    return <Pet />;
  }
  if (view === 'setup') {
    return (
      <Suspense fallback={null}>
        <Setup />
      </Suspense>
    );
  }
  return <LitePopup />;
}
