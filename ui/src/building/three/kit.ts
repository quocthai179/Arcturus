// Bộ dựng hình học thủ tục cho cảnh Tòa nhà: gom hộp/trụ/cầu có màu đỉnh vào vài mesh gộp (ít draw call) và một registry để
// `dispose()` đủ khi gỡ cảnh (docs/OFFICE_BUILDING.md §9.2–9.3). Chỉ file trong `three/` được import three.
import {
  BoxGeometry, BufferGeometry, Color, CylinderGeometry, DoubleSide, Euler, Float32BufferAttribute, Group, Matrix4, Mesh, MeshBasicMaterial,
  MeshLambertMaterial, Quaternion, SphereGeometry, Vector3, type Material,
} from 'three';
import { RoundedBoxGeometry } from 'three/examples/jsm/geometries/RoundedBoxGeometry.js';
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
  /** Tỉ lệ thêm theo từng trục (sau khi tạo hình) để làm hình elip, lá, đệm… */
  sx?: number;
  sy?: number;
  sz?: number;
  /** 0…1: tối dần phần chân vật (AO giả bằng màu đỉnh, không tốn ánh sáng). */
  ao?: number;
}

const tmpPos = new Vector3();
const tmpQuat = new Quaternion();
const tmpScale = new Vector3(1, 1, 1);
const tmpMat = new Matrix4();
const tmpEuler = new Euler();
const tmpColor = new Color();
const tmpDir = new Vector3();
const UP = new Vector3(0, 1, 0);

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
    this.addQ(g, x, y, z, color, p, tmpQuat, sx * (p.sx ?? 1), sy * (p.sy ?? 1), sz * (p.sz ?? 1));
  }

  private addQ(g: BufferGeometry, x: number, y: number, z: number, color: string, p: Place, quat: Quaternion, sx: number, sy: number, sz: number): void {
    g.deleteAttribute('uv');
    const pos = g.getAttribute('position');
    const n = pos.count;
    tmpColor.set(color);
    const colors = new Float32Array(n * 3);
    const ao = p.ao ?? 0;
    let minY = 0;
    let span = 1;
    if (ao > 0) {
      g.computeBoundingBox();
      minY = g.boundingBox?.min.y ?? 0;
      span = Math.max(1e-6, (g.boundingBox?.max.y ?? 1) - minY);
    }
    for (let i = 0; i < n; i++) {
      let k = 1;
      if (ao > 0) {
        const t = Math.min(1, (pos.getY(i) - minY) / span / 0.45);
        k = 1 - ao * (1 - t * t * (3 - 2 * t));
      }
      colors[i * 3] = tmpColor.r * k;
      colors[i * 3 + 1] = tmpColor.g * k;
      colors[i * 3 + 2] = tmpColor.b * k;
    }
    tmpPos.set(x, y, z);
    tmpScale.set(sx, sy, sz);
    tmpMat.compose(tmpPos, quat, tmpScale);
    g.applyMatrix4(tmpMat);
    g.setAttribute('color', new Float32BufferAttribute(colors, 3));
    this.parts[p.kind ?? 'lit'].push(g.index === null ? g : g.toNonIndexed());
  }

  /** Thêm một geometry bất kỳ (sẽ bị biến đổi tại chỗ và bỏ `uv`); `color` nhân vào màu đỉnh. */
  geo(g: BufferGeometry, x: number, y: number, z: number, color: string, p: Place = {}): this {
    this.add(g, x, y, z, color, p);
    return this;
  }

  /** Gộp riêng lớp `lit` thành một geometry (cho phần của nhân vật), không tạo `Mesh`. */
  geometry(): BufferGeometry {
    const list = this.parts.lit;
    const merged = list.length === 0 ? null : mergeGeometries(list, false);
    for (const g of list) g.dispose();
    this.parts.lit = [];
    return merged ?? new BufferGeometry();
  }

  /** Đưa một hình do người gọi dựng sẵn (đã có `position`, `normal`, `color`, không `uv`, không index) vào lô. */
  raw(g: BufferGeometry, kind: Kind = 'lit'): this {
    this.parts[kind].push(g);
    return this;
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
  /** Hộp bo góc (3×3 phân đoạn mỗi mặt, ~108 tam giác): đồ nội thất mềm mại thay cho hộp sắc cạnh. Bán kính ≤ 1,5 cm hoặc vật mỏng
   * (< 6 cm) thì bo không nhìn thấy nên dùng hộp thường (12 tam giác). */
  rbox(w: number, h: number, d: number, x: number, y: number, z: number, color: string, p: Place = {}, r = 0.05): this {
    if (r <= 0.015 || Math.min(w, h, d) < 0.06) return this.box(w, h, d, x, y, z, color, p);
    this.add(new RoundedBoxGeometry(w, h, d, 1, Math.max(0.002, Math.min(r, w / 2 - 0.001, h / 2 - 0.001, d / 2 - 0.001))), x, y, z, color, p);
    return this;
  }
  /** Thanh tròn nối hai điểm (dây treo, thân cây, tay đèn…). */
  link(a: readonly [number, number, number], b: readonly [number, number, number], r: number, color: string, p: Place = {}, seg = 6, rEnd = r): this {
    tmpDir.set(b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    const len = tmpDir.length();
    if (len < 1e-6) return this;
    tmpQuat.setFromUnitVectors(UP, tmpDir.normalize());
    this.addQ(new CylinderGeometry(rEnd, r, len, seg, 1), (a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2, color, p, tmpQuat, 1, 1, 1);
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


/** Bộ sinh số giả ngẫu nhiên xác định (dựng cảnh phải giống nhau mỗi lần). */
export function seeded(seed: number): () => number {
  let s = (seed >>> 0) % 2147483647 || 1;
  return () => (s = (s * 16807) % 2147483647) / 2147483647;
}

/** Sàn gỗ ván: mỗi ván một màu đỉnh trong `tones`, so le đầu ván, khe tối giữa các ván (ván nhỏ hơn ô 1,2 cm). Một geometry duy nhất. */
export function plankFloor(cx: number, cz: number, w: number, d: number, y: number, tones: readonly string[], seed: number, plankW = 0.3): BufferGeometry {
  const rnd = seeded(seed);
  const cols = tones.map((t) => new Color(t));
  const pos: number[] = [];
  const col: number[] = [];
  const rows = Math.max(1, Math.round(d / plankW));
  const rw = d / rows;
  for (let r = 0; r < rows; r++) {
    const z0 = cz - d / 2 + r * rw;
    let x = cx - w / 2 - rnd() * 1.6;
    while (x < cx + w / 2) {
      const len = 1.5 + rnd() * 1.5;
      const xa = Math.max(x, cx - w / 2);
      const xb = Math.min(x + len, cx + w / 2);
      x += len;
      if (xb - xa < 0.05) continue;
      const c = cols[Math.floor(rnd() * cols.length)] ?? cols[0];
      if (c === undefined) continue;
      const k = 0.94 + rnd() * 0.1;
      const gap = 0.012;
      const a = xa + gap / 2;
      const b = xb - gap / 2;
      const za = z0 + gap / 2;
      const zb = z0 + rw - gap / 2;
      // Hai tam giác hướng +y: (a,za)→(a,zb)→(b,zb) và (a,za)→(b,zb)→(b,za).
      pos.push(a, y, za, a, y, zb, b, y, zb, a, y, za, b, y, zb, b, y, za);
      for (let i = 0; i < 6; i++) col.push(c.r * k, c.g * k, c.b * k);
    }
  }
  const g = new BufferGeometry();
  g.setAttribute('position', new Float32BufferAttribute(pos, 3));
  const normals = new Float32Array(pos.length);
  for (let i = 1; i < normals.length; i += 3) normals[i] = 1;
  g.setAttribute('normal', new Float32BufferAttribute(normals, 3));
  g.setAttribute('color', new Float32BufferAttribute(col, 3));
  return g;
}

export type UvRect = readonly [number, number, number, number];

/** Gộp nhiều hình chữ nhật có UV (tranh trên tường, bóng đổ giả) thành một mesh, một draw call. */
export class QuadSet {
  private readonly pos: number[] = [];
  private readonly nrm: number[] = [];
  private readonly uv: number[] = [];

  private quad(c: Vector3, ax: Vector3, ay: Vector3, r: UvRect): void {
    const n = new Vector3().crossVectors(ax, ay).normalize();
    const v = (sx: number, sy: number): void => {
      this.pos.push(c.x + ax.x * sx + ay.x * sy, c.y + ax.y * sx + ay.y * sy, c.z + ax.z * sx + ay.z * sy);
      this.nrm.push(n.x, n.y, n.z);
      this.uv.push(sx < 0 ? r[0] : r[2], sy < 0 ? r[1] : r[3]);
    };
    v(-1, -1); v(1, -1); v(1, 1);
    v(-1, -1); v(1, 1); v(-1, 1);
  }

  /** Nằm ngang, mặt hướng +y; xoay `ry` quanh trục y. */
  flat(x: number, y: number, z: number, w: number, d: number, ry: number, r: UvRect): this {
    const cs = Math.cos(ry);
    const sn = Math.sin(ry);
    this.quad(new Vector3(x, y, z), new Vector3((w / 2) * cs, 0, (-w / 2) * sn), new Vector3((-d / 2) * sn, 0, (-d / 2) * cs), r);
    return this;
  }

  /** Dựng đứng, mặt hướng +z khi `ry` = 0. */
  wall(x: number, y: number, z: number, w: number, h: number, ry: number, r: UvRect): this {
    const cs = Math.cos(ry);
    const sn = Math.sin(ry);
    this.quad(new Vector3(x, y, z), new Vector3((w / 2) * cs, 0, (-w / 2) * sn), new Vector3(0, h / 2, 0), r);
    return this;
  }

  get count(): number {
    return this.pos.length / 18;
  }

  build(reg: Registry, material: Material): Mesh | null {
    if (this.pos.length === 0) return null;
    const g = reg.add(new BufferGeometry());
    g.setAttribute('position', new Float32BufferAttribute(this.pos, 3));
    g.setAttribute('normal', new Float32BufferAttribute(this.nrm, 3));
    g.setAttribute('uv', new Float32BufferAttribute(this.uv, 2));
    return new Mesh(g, material);
  }
}
