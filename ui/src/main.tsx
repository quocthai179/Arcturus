import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '@fontsource/be-vietnam-pro/400.css';
import '@fontsource/be-vietnam-pro/600.css';
import '@fontsource/jetbrains-mono/400.css';
import { App } from './App';
import './index.css';

const root = document.getElementById('root');
if (root === null) {
  throw new Error('Không thấy phần tử #root trong index.html');
}
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
