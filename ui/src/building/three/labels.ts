// Texture chữ cho cảnh Tòa nhà (CanvasTexture; không cần `blob:` nên không đổi CSP). Chuỗi không tin cậy chỉ được vẽ bằng
// `fillText` sau `cleanText()` (ký tự ẩn hiện thành ⟨U+XXXX⟩, cắt độ dài): không bao giờ vào DOM hay `innerHTML`.
import { CanvasTexture, LinearFilter, SRGBColorSpace, SpriteMaterial } from 'three';
import { cleanText } from '../../pet/brain';
import type { Registry } from './kit';

const FONT = "'Be Vietnam Pro', sans-serif";

/** Chờ phông nạp xong trước khi vẽ nhãn (thiếu thì dùng sans-serif). */
export function fontsReady(): Promise<void> {
  const fonts = typeof document !== 'undefined' ? document.fonts : undefined;
  return fonts === undefined ? Promise.resolve() : fonts.ready.then(() => undefined, () => undefined);
}

export function canvasTexture(reg: Registry, w: number, h: number, draw: (g: CanvasRenderingContext2D, w: number, h: number) => void): CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const g = canvas.getContext('2d');
  if (g !== null) draw(g, w, h);
  const t = reg.add(new CanvasTexture(canvas));
  t.colorSpace = SRGBColorSpace;
  t.minFilter = LinearFilter;
  t.generateMipmaps = false;
  return t;
}

export function redraw(t: CanvasTexture, draw: (g: CanvasRenderingContext2D, w: number, h: number) => void): void {
  const canvas = t.image as HTMLCanvasElement;
  const g = canvas.getContext('2d');
  if (g === null) return;
  g.clearRect(0, 0, canvas.width, canvas.height);
  draw(g, canvas.width, canvas.height);
  t.needsUpdate = true;
}

export function roundRect(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number): void {
  g.beginPath();
  g.moveTo(x + r, y);
  g.arcTo(x + w, y, x + w, y + h, r);
  g.arcTo(x + w, y + h, x, y + h, r);
  g.arcTo(x, y + h, x, y, r);
  g.arcTo(x, y, x + w, y, r);
  g.closePath();
}

/** Nhãn bàn 256×64: dòng 1 nhãn vai trò (danh sách trắng) hoặc "khách"/"phiên chính", dòng 2 tên (đã làm sạch). */
export function drawDeskLabel(g: CanvasRenderingContext2D, w: number, h: number, role: string, name: string, accent: string): void {
  g.fillStyle = 'rgba(12,14,20,0.82)';
  roundRect(g, 2, 2, w - 4, h - 4, 12);
  g.fill();
  g.fillStyle = accent;
  g.fillRect(2, 14, 5, h - 28);
  g.textAlign = 'center';
  g.textBaseline = 'middle';
  g.fillStyle = accent;
  g.font = `600 22px ${FONT}`;
  g.fillText(cleanText(role, 14), w / 2, 21, w - 24);
  g.fillStyle = '#E8EAF0';
  g.font = `500 22px ${FONT}`;
  g.fillText(cleanText(name, 16), w / 2, 46, w - 24);
}

export interface StatusScreen {
  floor: number;
  name: string;
  accent: string;
  busy: number;
  raised: number;
  errors: number;
  present: number;
  alert: 'none' | 'raised' | 'critical';
  topLine: string;
}

/** Màn hình tầng trên tường trạng thái (256×144): số tầng, tên, ▶ đang làm, ! giơ tay; viền cam khi có giơ tay, đỏ khi có Critical. */
export function drawStatusScreen(g: CanvasRenderingContext2D, w: number, h: number, s: StatusScreen): void {
  g.fillStyle = '#10131A';
  g.fillRect(0, 0, w, h);
  g.lineWidth = 8;
  g.strokeStyle = s.alert === 'critical' ? '#FF6B5A' : s.alert === 'raised' ? '#FFB020' : s.accent;
  g.strokeRect(4, 4, w - 8, h - 8);
  g.fillStyle = s.accent;
  g.globalAlpha = 0.18;
  g.fillRect(8, 8, w - 16, 44);
  g.globalAlpha = 1;
  g.textBaseline = 'middle';
  g.textAlign = 'left';
  g.fillStyle = s.accent;
  g.font = `700 36px ${FONT}`;
  g.fillText(String(s.floor), 20, 31);
  g.fillStyle = '#E8EAF0';
  g.font = `600 26px ${FONT}`;
  g.fillText(cleanText(s.name, 12), 62, 31, w - 80);
  g.font = `500 24px ${FONT}`;
  g.fillStyle = '#9BA6FF';
  g.fillText(`▶ ${s.busy} đang làm`, 20, 76, w - 40);
  g.fillStyle = s.raised > 0 ? '#FFB020' : '#7B8294';
  g.fillText(`! ${s.raised} giơ tay`, 20, 106, w - 40);
  if (s.errors > 0) {
    g.fillStyle = '#FF6B5A';
    g.textAlign = 'right';
    g.fillText(`${s.errors} lỗi`, w - 18, 106);
  }
}

export function drawBang(g: CanvasRenderingContext2D, w: number, h: number): void {
  g.fillStyle = '#FFB020';
  g.beginPath();
  g.arc(w / 2, h / 2, w * 0.46, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = '#1D1500';
  g.font = `800 ${Math.round(h * 0.7)}px ${FONT}`;
  g.textAlign = 'center';
  g.textBaseline = 'middle';
  g.fillText('!', w / 2, h / 2 + h * 0.04);
}

export function drawDots(g: CanvasRenderingContext2D, w: number, h: number): void {
  g.fillStyle = '#FFFDF6';
  roundRect(g, 4, 8, w - 8, h - 24, 14);
  g.fill();
  g.beginPath();
  g.moveTo(w / 2 - 8, h - 16);
  g.lineTo(w / 2, h - 3);
  g.lineTo(w / 2 + 8, h - 16);
  g.fill();
  g.fillStyle = '#1D1F27';
  for (const k of [-1, 0, 1]) {
    g.beginPath();
    g.arc(w / 2 + k * 16, (h - 16) / 2 + 8, 5, 0, Math.PI * 2);
    g.fill();
  }
}

export function spriteMaterial(reg: Registry, t: CanvasTexture): SpriteMaterial {
  return reg.add(new SpriteMaterial({ map: t, transparent: true, depthWrite: false, depthTest: false, toneMapped: false }));
}

export function drawSign(g: CanvasRenderingContext2D, w: number, h: number, text: string, color: string, bg = 'rgba(0,0,0,0)'): void {
  g.fillStyle = bg;
  g.fillRect(0, 0, w, h);
  g.fillStyle = color;
  g.font = `800 ${Math.round(h * 0.62)}px ${FONT}`;
  g.textAlign = 'center';
  g.textBaseline = 'middle';
  g.fillText(text, w / 2, h / 2 + h * 0.03, w - 20);
}
