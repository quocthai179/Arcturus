// Bộ dựng hình học thủ tục cho cảnh Tòa nhà: gom hộp/trụ/cầu có màu đỉnh vào vài mesh gộp (ít draw call) và một registry để
// `dispose()` đủ khi gỡ cảnh (docs/OFFICE_BUILDING.md §9.2–9.3). Chỉ file trong `three/` được import three.
import {
  BoxGeometry, BufferGeometry, Color, CylinderGeometry, DoubleSide, Euler, Float32BufferAttribute, Group, Matrix4, Mesh, MeshBasicMaterial,
  MeshLambertMaterial, Quaternion, SphereGeometry, Vector3, type Material,
} from 'three';
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js';

export interface Disposable {
  dispose(): void;
}

/** Mọi geometry/material/texture đăng ký ở đây; `disposeAll()` khi gỡ (hoặc rời tầng). */
export class Registry {
  private readonly items = new Set<Disposable>();
  add<T extends Disposable>(item: T): T {
    this.items.add(item);
    return item;
  }
  disposeAll(): void {
    for (const i of this.items) i.dispose();
    this.items.clear();
  }
  get size(): number {
    return this.items.size;
  }
}

export type Kind = 'lit' | 'glow' | 'glass';
export interface Place {
  ry?: number;
  rx?: number;
  rz?: number;
  kind?: Kind;
}

const tmpPos = new Vector3();
const tmpQuat = new Quaternion();
const tmpScale = new Vector3(1, 1, 1);
const tmpMat = new Matrix4();
const tmpEuler = new Euler();
const tmpColor = new Color();

export interface Materials {
  lit: Material;
  glow: Material;
  glass: Material;
}

/** Gom hình học theo loại vật liệu rồi gộp mỗi loại thành một `Mesh`. */
export class Batch {
  private readonly parts: Record<Kind, BufferGeometry[]> = { lit: [], glow: [], glass: [] };

  private add(g: BufferGeometry, x: number, y: number, z: number, color: string, p: Place, sx = 1, sy = 1, sz = 1): void {
    tmpEuler.set(p.rx ?? 0, p.ry ?? 0, p.rz ?? 0, 'YXZ');
    tmpQuat.setFromEuler(tmpEuler);
    tmpPos.set(x, y, z);
    tmpScale.set(sx, sy, sz);
    tmpMat.compose(tmpPos, tmpQuat, tmpScale);
    g.applyMatrix4(tmpMat);
    g.deleteAttribute('uv');
    const n = g.getAttribute('position').count;
    tmpColor.set(color);
    const colors = new Float32Array(n * 3);
    for (let i = 0; i < n; i++) {
      colors[i * 3] = tmpColor.r;
      colors[i * 3 + 1] = tmpColor.g;
      colors[i * 3 + 2] = tmpColor.b;
    }
    g.setAttribute('color', new Float32BufferAttribute(colors, 3));
    this.parts[p.kind ?? 'lit'].push(g.index === null ? g : g.toNonIndexed());
  }

  box(w: number, h: number, d: number, x: number, y: number, z: number, color: string, p: Place = {}): this {
    this.add(new BoxGeometry(w, h, d), x, y, z, color, p);
    return this;
  }
  /** Trụ đứng, tâm ở (x, y, z). */
  cyl(rTop: number, rBottom: number, h: number, x: number, y: number, z: number, color: string, p: Place = {}, seg = 12): this {
    this.add(new CylinderGeometry(rTop, rBottom, h, seg), x, y, z, color, p);
    return this;
  }
  sph(r: number, x: number, y: number, z: number, color: string, p: Place = {}, sy = 1, seg = 12): this {
    this.add(new SphereGeometry(r, seg, Math.max(6, seg - 4)), x, y, z, color, p, 1, sy, 1);
    return this;
  }

  /** Gộp thành một Group (một Mesh mỗi loại có dữ liệu). `reg` nhận geometry để dispose. */
  build(reg: Registry, mats: Materials, shadows = true): Group {
    const group = new Group();
    for (const kind of ['lit', 'glow', 'glass'] as const) {
      const list = this.parts[kind];
      if (list.length === 0) continue;
      const merged = mergeGeometries(list, false);
      for (const g of list) g.dispose();
      if (merged === null) continue;
      reg.add(merged);
      const mesh = new Mesh(merged, mats[kind]);
      mesh.castShadow = shadows && kind === 'lit';
      mesh.receiveShadow = kind === 'lit';
      mesh.renderOrder = kind === 'glass' ? 2 : 0;
      group.add(mesh);
    }
    this.parts.lit = [];
    this.parts.glow = [];
    this.parts.glass = [];
    return group;
  }
}

export function makeMaterials(reg: Registry, glassOpacity = 0.3): Materials & { glassMat: MeshLambertMaterial } {
  const lit = reg.add(new MeshLambertMaterial({ vertexColors: true }));
  const glow = reg.add(new MeshBasicMaterial({ vertexColors: true }));
  const glassMat = reg.add(new MeshLambertMaterial({ vertexColors: true, transparent: true, opacity: glassOpacity, depthWrite: false, side: DoubleSide }));
  return { lit, glow, glass: glassMat, glassMat };
}

export function hexToRgb(hex: string): [number, number, number] {
  const c = new Color(hex);
  return [c.r, c.g, c.b];
}

