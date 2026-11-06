// Texture trang trí dùng chung (một atlas 512×512 cho mọi tranh/bảng, một blob bóng đổ 64×64) + bảng vùng UV. Toàn bộ vẽ bằng canvas
// 2D, không tải gì từ ngoài. Chữ trên tranh là chuỗi cố định trong mã (không có dữ liệu từ agent).
import type { CanvasTexture } from 'three';
import type { Registry, UvRect } from './kit';
import { canvasTexture, roundRect } from './labels';
import { PAL } from './palette';

export type ArtId = 'artArcs' | 'artSun' | 'artHills' | 'artShapes' | 'artLeaf' | 'artHello' | 'posterRocket' | 'posterStripes' | 'whiteboardNotes' | 'cork' | 'whiteboardFlow' | 'roadmap';

const ATLAS = 512;
/** [x, y, w, h] theo pixel canvas. */
const CELLS: Record<ArtId, readonly [number, number, number, number]> = {
  artArcs: [0, 0, 128, 128],
  artSun: [128, 0, 128, 128],
  artHills: [256, 0, 128, 128],
  artShapes: [384, 0, 128, 128],
  artLeaf: [0, 128, 128, 128],
  artHello: [128, 128, 128, 128],
  posterRocket: [256, 128, 128, 128],
  posterStripes: [384, 128, 128, 128],
  whiteboardNotes: [0, 256, 256, 128],
  cork: [256, 256, 256, 128],
  whiteboardFlow: [0, 384, 256, 128],
  roadmap: [256, 384, 256, 128],
};

export function artRect(id: ArtId): UvRect {
  const [x, y, w, h] = CELLS[id];
  const inset = 1 / ATLAS;
  return [x / ATLAS + inset, 1 - (y + h) / ATLAS + inset, (x + w) / ATLAS - inset, 1 - y / ATLAS - inset];
}

export const ART_IDS = Object.keys(CELLS) as ArtId[];

function cell(g: CanvasRenderingContext2D, id: ArtId, paint: (w: number, h: number) => void): void {
  const [x, y, w, h] = CELLS[id];
  g.save();
  g.translate(x, y);
  g.beginPath();
  g.rect(0, 0, w, h);
  g.clip();
  paint(w, h);
  g.restore();
}

const NOTE = ['#FFE28A', '#FFB3A7', '#A8E0C8', '#B7D3FF', '#F7C6E8'];

function notes(g: CanvasRenderingContext2D, x0: number, y0: number, cols: number, rows: number, s: number, gap: number, seed: number): void {
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const k = (r * 7 + c * 3 + seed) % NOTE.length;
      g.save();
      g.translate(x0 + c * (s + gap) + s / 2, y0 + r * (s + gap) + s / 2);
      g.rotate((((r + c * 2 + seed) % 5) - 2) * 0.04);
      g.fillStyle = NOTE[k] ?? '#FFE28A';
      g.fillRect(-s / 2, -s / 2, s, s);
      g.fillStyle = 'rgba(60,50,40,0.45)';
      g.fillRect(-s / 2 + 4, -s / 2 + s * 0.3, s * 0.6, 2.5);
      g.fillRect(-s / 2 + 4, -s / 2 + s * 0.5, s * 0.45, 2.5);
      g.restore();
    }
  }
}

export function artAtlas(reg: Registry): CanvasTexture {
  return canvasTexture(reg, ATLAS, ATLAS, (g) => {
    g.fillStyle = PAL.cream;
    g.fillRect(0, 0, ATLAS, ATLAS);
    cell(g, 'artArcs', (w, h) => {
      g.fillStyle = '#F2E4D0';
      g.fillRect(0, 0, w, h);
      const cols = [PAL.terracotta, PAL.mustard, PAL.sage, PAL.dustyBlue];
      cols.forEach((c, i) => {
        g.strokeStyle = c;
        g.lineWidth = 12;
        g.beginPath();
        g.arc(w / 2, h * 0.9, 20 + i * 20, Math.PI, 2 * Math.PI);
        g.stroke();
      });
    });
    cell(g, 'artSun', (w, h) => {
      const grad = g.createLinearGradient(0, 0, 0, h);
      grad.addColorStop(0, '#F6C79A');
      grad.addColorStop(1, '#E58E78');
      g.fillStyle = grad;
      g.fillRect(0, 0, w, h);
      g.fillStyle = '#FFF1D0';
      g.beginPath();
      g.arc(w / 2, h * 0.52, 30, 0, Math.PI * 2);
      g.fill();
      g.fillStyle = '#7B5A82';
      g.fillRect(0, h * 0.7, w, h * 0.3);
    });
    cell(g, 'artHills', (w, h) => {
      g.fillStyle = '#CFE3EA';
      g.fillRect(0, 0, w, h);
      for (const [c, y, a] of [['#9CC1A6', 0.5, 0.2], ['#6FA38A', 0.62, 0.35], ['#3F7A64', 0.78, 0.5]] as const) {
        g.fillStyle = c;
        g.beginPath();
        g.moveTo(0, h);
        g.lineTo(0, h * y);
        g.quadraticCurveTo(w * a, h * (y - 0.25), w * 0.6, h * y);
        g.quadraticCurveTo(w * 0.85, h * (y + 0.1), w, h * (y - 0.05));
        g.lineTo(w, h);
        g.fill();
      }
    });
    cell(g, 'artShapes', (w, h) => {
      g.fillStyle = '#F7F1E6';
      g.fillRect(0, 0, w, h);
      g.fillStyle = PAL.dustyBlue;
      g.beginPath();
      g.arc(46, 52, 28, 0, Math.PI * 2);
      g.fill();
      g.fillStyle = PAL.mustard;
      g.fillRect(62, 56, 48, 48);
      g.fillStyle = PAL.terracotta;
      g.beginPath();
      g.moveTo(30, 108);
      g.lineTo(62, 62);
      g.lineTo(94, 108);
      g.fill();
    });
    cell(g, 'artLeaf', (w, h) => {
      g.fillStyle = '#F4EDE0';
      g.fillRect(0, 0, w, h);
      g.strokeStyle = '#3F7A5A';
      g.lineWidth = 3;
      g.beginPath();
      g.moveTo(w / 2, h - 14);
      g.lineTo(w / 2, 24);
      g.stroke();
      for (let i = 0; i < 5; i++) {
        for (const s of [-1, 1]) {
          g.fillStyle = i % 2 === 0 ? '#5E9E6E' : '#3F7A5A';
          g.beginPath();
          g.ellipse(w / 2 + s * 20, 36 + i * 18, 20, 8, s * -0.5, 0, Math.PI * 2);
          g.fill();
        }
      }
    });
    cell(g, 'artHello', (w, h) => {
      g.fillStyle = '#2F3A4B';
      g.fillRect(0, 0, w, h);
      g.fillStyle = '#FFE2A8';
      g.font = '800 30px sans-serif';
      g.textAlign = 'center';
      g.textBaseline = 'middle';
      g.fillText('hello', w / 2, h / 2 - 12);
      g.fillStyle = '#F2A58E';
      g.fillText('world', w / 2, h / 2 + 22);
    });
    cell(g, 'posterRocket', (w, h) => {
      g.fillStyle = '#26324A';
      g.fillRect(0, 0, w, h);
      g.fillStyle = '#FFF';
      for (let i = 0; i < 14; i++) g.fillRect((i * 37) % w, (i * 53) % h, 2, 2);
      g.fillStyle = '#F4F0EA';
      g.beginPath();
      g.ellipse(w / 2, h / 2, 14, 34, 0, 0, Math.PI * 2);
      g.fill();
      g.fillStyle = '#E8735C';
      g.beginPath();
      g.moveTo(w / 2 - 14, h / 2 + 18);
      g.lineTo(w / 2 - 28, h / 2 + 44);
      g.lineTo(w / 2 - 8, h / 2 + 34);
      g.fill();
      g.beginPath();
      g.moveTo(w / 2 + 14, h / 2 + 18);
      g.lineTo(w / 2 + 28, h / 2 + 44);
      g.lineTo(w / 2 + 8, h / 2 + 34);
      g.fill();
      g.fillStyle = '#FFD166';
      g.beginPath();
      g.arc(w / 2, h / 2 - 6, 7, 0, Math.PI * 2);
      g.fill();
    });
    cell(g, 'posterStripes', (w, h) => {
      const cols = [PAL.oat, PAL.terracotta, PAL.mustard, PAL.sage, PAL.dustyBlue, PAL.cream];
      cols.forEach((c, i) => {
        g.fillStyle = c;
        g.fillRect(0, (i * h) / cols.length, w, h / cols.length + 1);
      });
    });
    cell(g, 'whiteboardNotes', (w, h) => {
      g.fillStyle = '#FAFAF7';
      g.fillRect(0, 0, w, h);
      g.strokeStyle = '#4C7BD9';
      g.lineWidth = 3;
      g.beginPath();
      g.moveTo(14, 20);
      g.lineTo(90, 20);
      g.moveTo(14, 34);
      g.lineTo(60, 34);
      g.stroke();
      notes(g, 110, 12, 5, 3, 26, 6, 1);
      notes(g, 14, 60, 3, 2, 26, 6, 3);
    });
    cell(g, 'cork', (w, h) => {
      g.fillStyle = '#C9A06E';
      g.fillRect(0, 0, w, h);
      g.fillStyle = 'rgba(120,80,40,0.25)';
      for (let i = 0; i < 90; i++) g.fillRect((i * 53) % w, (i * 29) % h, 2, 2);
      notes(g, 12, 12, 6, 3, 28, 10, 2);
      g.fillStyle = '#FAFAF7';
      g.fillRect(190, 20, 50, 70);
      g.fillStyle = '#E8735C';
      g.fillRect(196, 28, 38, 6);
    });
    cell(g, 'whiteboardFlow', (w, h) => {
      g.fillStyle = '#FAFAF7';
      g.fillRect(0, 0, w, h);
      g.strokeStyle = '#4C7BD9';
      g.fillStyle = '#E8F0FF';
      g.lineWidth = 3;
      for (const x of [20, 100, 180]) {
        roundRect(g, x, 40, 56, 40, 8);
        g.fill();
        g.stroke();
      }
      g.strokeStyle = '#E8735C';
      g.beginPath();
      g.moveTo(76, 60);
      g.lineTo(100, 60);
      g.moveTo(156, 60);
      g.lineTo(180, 60);
      g.stroke();
      g.strokeStyle = '#34A58C';
      g.beginPath();
      g.moveTo(208, 82);
      g.quadraticCurveTo(208, 112, 128, 112);
      g.quadraticCurveTo(48, 112, 48, 82);
      g.stroke();
    });
    cell(g, 'roadmap', (w, h) => {
      g.fillStyle = '#F7F4EE';
      g.fillRect(0, 0, w, h);
      const cols = [PAL.dustyBlue, PAL.sage, PAL.mustard, PAL.terracotta];
      cols.forEach((c, i) => {
        g.fillStyle = c;
        roundRect(g, 12 + i * 20, 16 + i * 26, 120 + (i % 2) * 70, 18, 6);
        g.fill();
      });
      g.fillStyle = '#C9C2B6';
      for (let i = 1; i < 6; i++) g.fillRect(i * 40, 8, 1, h - 16);
    });
  });
}

/** Blob bóng đổ giả: tâm đen mờ dần ra rìa (alpha). */
export function blobTexture(reg: Registry): CanvasTexture {
  return canvasTexture(reg, 64, 64, (g, w, h) => {
    const grad = g.createRadialGradient(w / 2, h / 2, 2, w / 2, h / 2, w / 2);
    grad.addColorStop(0, 'rgba(30,18,10,0.62)');
    grad.addColorStop(0.55, 'rgba(30,18,10,0.3)');
    grad.addColorStop(1, 'rgba(30,18,10,0)');
    g.fillStyle = grad;
    g.fillRect(0, 0, w, h);
  });
}
