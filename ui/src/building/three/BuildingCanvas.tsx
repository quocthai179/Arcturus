// Bộ vẽ three.js của cảnh Tòa nhà (docs/OFFICE_BUILDING.md §9). Nạp muộn bằng `React.lazy` nên three không vào bundle chính.
// Vẽ theo nhu cầu: KHÔNG có vòng `requestAnimationFrame` thường trực; một khung khi dữ liệu/kích thước/chuột đổi, vòng ngắn chỉ trong
// tween (chuyển tầng ≤30 FPS, thư ký đi ≤15 FPS). Gỡ component thì `dispose()` mọi geometry/material/texture và `forceContextLoss()`.
// Canvas KHÔNG biết `onSelect`/`brief`: chỉ báo vật được chọn (`onHover`/`onPick`); vỏ DOM quyết định phần còn lại.
import { useEffect, useRef } from 'react';
import {
  InstancedMesh, Matrix4, Mesh, MeshBasicMaterial, Object3D, PerspectiveCamera, PCFShadowMap, PlaneGeometry, Raycaster, SRGBColorSpace, Vector2, Vector3, WebGLRenderer, Color,
} from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import type { Mood } from '../../protocol';
import { ease, FOV, frameFor, NAV_MS, viewTargets, type View } from '../camera';
import { FLOORS, FLOOR_COUNT, type FloorId } from '../floors';
import { BASE, deskAt, deskCount, GEO, LIFT, ROOF_Y, toWorld } from '../layout';
import { planPrune } from '../interiorCache';
import { createLoop, type Loop } from '../loop';
import type { BuildingCanvasProps, BuildingModel, PickHit } from '../model';
import { routePosAt } from '../route';
import { secretaryPose } from '../secretary';
import { createFigures, type Anchors, type Figures } from './figures';
import { buildInterior, SCREEN_COLORS, type Interior } from './interiors';
import { Registry } from './kit';
import type { StatusScreen } from './labels';
import { buildWorld, type World } from './world';

const HOVER_EDGE = 1.7;
const IDLE_EDGE = 0.65;
const CLICK_SLOP = 5;

interface State {
  model: BuildingModel;
  motion: boolean;
  lifts: number[];
  cuts: number[];
  interiors: Map<FloorId, Interior>;
  recent: FloorId[];
  carY: number;
  secKey: number | null;
  secT: number | null;
  anchors: Anchors | null;
  hoverFloor: number | null;
  shadowDirty: boolean;
  firstFrame: boolean;
  navigating: boolean;
  navId: number;
}

export default function BuildingCanvas(props: BuildingCanvasProps) {
  const host = useRef<HTMLDivElement>(null);
  const latest = useRef(props);
  latest.current = props;
  const api = useRef<{ apply: (m: BuildingModel) => void; motion: (on: boolean) => void } | null>(null);

  useEffect(() => {
    const el = host.current;
    if (el === null) return undefined;
    let renderer: WebGLRenderer;
    try {
      renderer = new WebGLRenderer({ antialias: true, powerPreference: 'low-power' });
    } catch (e) {
      latest.current.onError(e instanceof Error ? e.message : 'Không tạo được WebGL renderer');
      return undefined;
    }
    const reg = new Registry();
    const canvas = renderer.domElement;
    try {
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.5));
    renderer.outputColorSpace = SRGBColorSpace;
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = PCFShadowMap;
    renderer.shadowMap.autoUpdate = false;
    el.appendChild(canvas);

    const world: World = buildWorld(reg);
    const figures: Figures = createFigures();
    world.scene.add(figures.group);
    const camera = new PerspectiveCamera(FOV, 1, 0.5, 600);
    const controls = new OrbitControls(camera, canvas);
    controls.enablePan = false;
    controls.enableDamping = false;
    controls.minDistance = 9;
    controls.maxDistance = 110;
    controls.minPolarAngle = 0.15;
    controls.maxPolarAngle = 1.48;
    controls.rotateSpeed = 0.7;

    // Ô sáng của bàn cho tầng chưa dựng nội thất (ngoại cảnh): một InstancedMesh cho cả tòa.
    const glowDesks: { floor: FloorId; index: number }[] = [];
    for (const f of [0, 1, 2, 3, 4] as const) for (let i = 0; i < deskCount(f); i++) glowDesks.push({ floor: f, index: i });
    const glowMat = reg.add(new MeshBasicMaterial({ color: '#FFFFFF', toneMapped: false }));
    const glow = new InstancedMesh(reg.add(new PlaneGeometry(0.58, 0.34)), glowMat, glowDesks.length);
    glow.frustumCulled = false;
    for (let i = 0; i < glowDesks.length; i++) glow.setColorAt(i, new Color('#12141A'));
    world.scene.add(glow);

    const S: State = {
      model: latest.current.model,
      motion: latest.current.motion,
      lifts: viewTargets(latest.current.model.view).lift.slice(),
      cuts: viewTargets(latest.current.model.view).cut.slice(),
      interiors: new Map(),
      recent: [],
      carY: (BASE[5] ?? 0) + 0.3,
      secKey: null,
      secT: null,
      anchors: null,
      hoverFloor: null,
      shadowDirty: true,
      firstFrame: true,
      navigating: false,
      navId: 0,
    };
    let size = { w: 1, h: 1 };
    let disposed = false;

    // --- nội thất theo tầng đang xem ---
    const ensureInterior = (f: FloorId) => {
      if (S.interiors.has(f)) return;
      const it = buildInterior(f, world.mats);
      world.floors[f]?.group.add(it.group);
      world.floors[f]?.top.add(it.top);
      S.interiors.set(f, it);
      S.shadowDirty = true;
    };
    const prune = (navigatedTo: View) => {
      const plan = planPrune(S.interiors.keys(), S.recent, navigatedTo, S.model.view);
      S.recent = plan.recent;
      for (const f of plan.dispose) {
        const it = S.interiors.get(f);
        if (it === undefined) continue;
        world.floors[f]?.group.remove(it.group);
        world.floors[f]?.top.remove(it.top);
        it.dispose();
        S.interiors.delete(f);
      }
    };

    const loop: Loop = createLoop(render);

    function applyState(): void {
      for (let n = 0; n < FLOOR_COUNT; n++) {
        const node = world.floors[n];
        const g = GEO[n];
        if (node === undefined || g === undefined) continue;
        const lift = S.lifts[n] ?? 0;
        node.group.position.set(g.cx, (BASE[n] ?? 0) + lift, g.cz);
        node.group.visible = lift < 0.45 * LIFT;
        const cut = S.cuts[n] ?? 0;
        node.glass.opacity = (n === 5 ? 0.14 : 0.3) * (1 - cut) + 0.06 * cut;
        node.top.visible = cut < 0.5;
      }
      const g5 = GEO[5];
      if (g5 !== undefined) world.roof.position.set(g5.cx, ROOF_Y + (S.lifts[6] ?? 0), g5.cz);
      world.roof.visible = (S.lifts[6] ?? 0) < 0.45 * LIFT;
      for (const node of world.floors) node.edge.color.setScalar(IDLE_EDGE);
      if (S.hoverFloor !== null) world.floors[S.hoverFloor]?.edge.color.setScalar(HOVER_EDGE);
    }

    function syncData(): void {
      const { rosters, pending } = S.model;
      const moodsOf = (f: FloorId): (Mood | null)[] => {
        const n = f === 5 ? 0 : deskCount(f);
        const moods: (Mood | null)[] = Array.from({ length: n }, () => null);
        const roster = f === 5 ? undefined : rosters[f];
        for (const fig of roster?.figures ?? []) if (fig.spot.kind === 'desk') moods[fig.spot.index] = fig.agent.mood;
        return moods;
      };
      for (const [f, it] of S.interiors) {
        if (f === 5) {
          const screens: StatusScreen[] = rosters.map((r) => {
            const critical = r.figures.some((x) => pending.get(x.agent.id)?.risk === 'critical');
            const def = FLOORS[r.floor];
            return {
              floor: r.floor, name: def?.name ?? '', accent: def?.accent ?? '#9AA3B5', busy: r.stats.busy, raised: r.stats.raised, errors: r.stats.errors,
              present: r.stats.present, alert: critical ? 'critical' : r.stats.raised > 0 ? 'raised' : 'none', topLine: '',
            };
          });
          it.setStatus(screens);
        } else {
          it.setDeskStates(moodsOf(f));
        }
      }
      const tmp = new Color();
      const o = new Object3D();
      glowDesks.forEach((d, i) => {
        const moods = moodsOf(d.floor);
        const mood = moods[d.index] ?? null;
        const built = S.interiors.has(d.floor);
        const p = deskAt(d.floor as 0 | 1 | 2 | 3 | 4, d.index).desk;
        const w = toWorld(d.floor, { x: p.x - 0.1, y: 0.3 + 0.72 + 0.34, z: p.z - 0.12 });
        o.position.set(w.x, w.y + (S.lifts[d.floor] ?? 0), w.z);
        o.scale.setScalar(built ? 0 : 1);
        o.updateMatrix();
        glow.setMatrixAt(i, o.matrix);
        tmp.set(mood === null ? '#12141A' : SCREEN_COLORS[mood]);
        if (mood === 'idle') tmp.multiplyScalar(0.55);
        glow.setColorAt(i, tmp);
      });
      glow.instanceMatrix.needsUpdate = true;
      if (glow.instanceColor !== null) glow.instanceColor.needsUpdate = true;
    }

    function secretaryNow(): { t: number } | null {
      const st = S.model.secretary.mem.state;
      if (st.k !== 'moving') return null;
      return { t: S.secT ?? Date.now() - st.startAt };
    }

    function render(): void {
      if (disposed || document.hidden) return;
      applyState();
      syncGlowLifts();
      const mem = S.model.secretary.mem;
      const sn = secretaryNow();
      const st = mem.state;
      const at = st.k === 'moving' && sn !== null ? st.startAt + sn.t : Date.now();
      const pose = secretaryPose(mem, at);
      let phase = 0;
      if (st.k === 'moving' && sn !== null) {
        const rp = routePosAt(st.route, sn.t);
        phase = rp.phase;
        if (rp.carY !== null) S.carY = rp.carY;
      } else if (st.k === 'briefing') {
        // cabin đứng yên ở tầng cuối thư ký tới
        S.carY = (BASE[st.at.floor] ?? 0) + 0.3;
      }
      world.car.position.y = S.carY;
      const detailFloor = typeof S.model.view === 'number' ? S.model.view : null;
      S.anchors = figures.update({
        detailFloor, rosters: S.model.rosters, lifts: S.lifts, selectable: S.model.selectable,
        secretary: { floor: pose.floor, pos: pose.pos, pose: pose.pose, facing: pose.facing, phase, speaking: mem.bubble !== null },
      });
      if (S.shadowDirty) {
        renderer.shadowMap.needsUpdate = true;
        S.shadowDirty = false;
      }
      renderer.render(world.scene, camera);
      if (S.firstFrame) {
        S.firstFrame = false;
        latest.current.onReady();
      }
    }

    const glowObj = new Object3D();
    const glowMat4 = new Matrix4();
    function syncGlowLifts(): void {
      const o = glowObj;
      const m = glowMat4;
      glowDesks.forEach((d, i) => {
        glow.getMatrixAt(i, m);
        if (m.elements[0] === 0) return;
        const p = deskAt(d.floor as 0 | 1 | 2 | 3 | 4, d.index).desk;
        const w = toWorld(d.floor, { x: p.x - 0.1, y: 0.3 + 0.72 + 0.34, z: p.z - 0.12 });
        o.position.set(w.x, w.y + (S.lifts[d.floor] ?? 0), w.z);
        o.scale.set(1, 1, 1);
        o.updateMatrix();
        glow.setMatrixAt(i, o.matrix);
      });
      glow.instanceMatrix.needsUpdate = true;
      glow.visible = S.model.view === 'ext' || S.interiors.size < 6;
    }

    // --- chuyển góc nhìn ---
    const aspect = () => size.w / Math.max(1, size.h);
    function setCamera(posV: Vector3, targetV: Vector3): void {
      camera.position.copy(posV);
      controls.target.copy(targetV);
      camera.lookAt(targetV);
    }
    function goView(view: View, animate: boolean): void {
      if (typeof view === 'number') ensureInterior(view);
      syncData();
      const tg = viewTargets(view);
      const fr = frameFor(view, aspect());
      const l0 = S.lifts.slice();
      const c0 = S.cuts.slice();
      const p0 = camera.position.clone();
      const t0 = controls.target.clone();
      const p1 = new Vector3(fr.pos.x, fr.pos.y, fr.pos.z);
      const t1 = new Vector3(fr.target.x, fr.target.y, fr.target.z);
      const token = ++S.navId;
      S.navigating = true;
      const step = (p: number) => {
        const e = ease(p);
        S.lifts = l0.map((v, i) => v + ((tg.lift[i] ?? 0) - v) * e);
        S.cuts = c0.map((v, i) => v + ((tg.cut[i] ?? 0) - v) * e);
        setCamera(p0.clone().lerp(p1, e), t0.clone().lerp(t1, e));
      };
      void loop.tween(animate ? NAV_MS : 0, 30, step, 'nav').then(() => {
        if (disposed || token !== S.navId) return;
        step(1);
        S.navigating = false;
        if (typeof S.model.view === 'number') ensureInterior(S.model.view);
        prune(view);
        syncData();
        S.shadowDirty = true;
        loop.invalidate();
      });
    }

    function secretaryTween(): void {
      const st = S.model.secretary.mem.state;
      if (st.k !== 'moving') {
        S.secKey = null;
        S.secT = null;
        return;
      }
      if (S.secKey === st.startAt) return;
      S.secKey = st.startAt;
      const key = st.startAt;
      const elapsed = Math.max(0, Date.now() - st.startAt);
      const remaining = Math.max(0, st.route.totalMs - elapsed);
      S.secT = elapsed;
      void loop.tween(remaining, 15, (p) => {
        if (S.secKey === key) S.secT = elapsed + p * remaining;
      }, 'sec').then(() => {
        if (S.secKey === key) S.secT = st.route.totalMs;
        loop.invalidate();
      });
    }

    // --- model từ vỏ ---
    api.current = {
      apply(model) {
        const prev = S.model;
        S.model = model;
        if (prev.timeMode !== model.timeMode) world.setTime(model.timeMode);
        if (prev.view !== model.view) goView(model.view, true);
        else syncData();
        secretaryTween();
        S.shadowDirty = true;
        loop.invalidate();
      },
      motion(on) {
        S.motion = on;
        loop.setMotion(on);
        if (on) loop.invalidate();
      },
    };

    // --- kích thước ---
    const resize = () => {
      const w = Math.max(1, el.clientWidth);
      const h = Math.max(1, el.clientHeight);
      size = { w, h };
      renderer.setSize(w, h, false);
      canvas.style.width = '100%';
      canvas.style.height = '100%';
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      if (!S.navigating) {
        const fr = frameFor(S.model.view, w / h);
        setCamera(new Vector3(fr.pos.x, fr.pos.y, fr.pos.z), new Vector3(fr.target.x, fr.target.y, fr.target.z));
      }
      loop.invalidate();
    };
    const ro = new ResizeObserver(resize);
    ro.observe(el);

    // --- chọn vật ---
    const ray = new Raycaster();
    const ndc = new Vector2();
    const project = (v: Vector3): { x: number; y: number } => {
      const p = v.clone().project(camera);
      return { x: ((p.x + 1) / 2) * size.w, y: ((1 - p.y) / 2) * size.h };
    };
    function pick(cx: number, cy: number): PickHit | null {
      const r = canvas.getBoundingClientRect();
      const x = cx - r.left;
      const y = cy - r.top;
      ndc.set((x / r.width) * 2 - 1, -(y / r.height) * 2 + 1);
      ray.setFromCamera(ndc, camera);
      const view = S.model.view;
      const a = S.anchors;
      if (typeof view === 'number' && a !== null) {
        const bang = ray.intersectObjects(a.bangs.filter((s) => s.visible), false)[0]?.object;
        const bangId = bang?.userData['agentId'];
        if (typeof bangId === 'string') return { k: 'bang', agentId: bangId };
        if (a.secretary !== null) {
          const p = project(a.secretary.clone().setY(a.secretary.y - 0.45));
          if (Math.hypot(p.x - x, p.y - y) < 44) return { k: 'secretary' };
        }
        if (view === 5) {
          const meshes = S.interiors.get(5)?.statusMeshes ?? [];
          const hit = ray.intersectObjects([...meshes], false)[0]?.object;
          const f = hit?.userData['statusFloor'];
          if (typeof f === 'number' && f >= 0 && f <= 4) return { k: 'status-screen', floor: f as 0 | 1 | 2 | 3 | 4 };
        }
        let best: { id: string; d: number } | null = null;
        for (const [id, head] of a.agents) {
          const p = project(head.clone().setY(head.y - 0.45));
          const d = Math.hypot(p.x - x, p.y - y);
          if (d < 30 && (best === null || d < best.d)) best = { id, d };
        }
        if (best !== null) return { k: 'agent', agentId: best.id };
      }
      const picks: Mesh[] = [];
      world.floors.forEach((node, n) => {
        if (node.group.visible && n !== view) picks.push(node.pick);
      });
      const hit = ray.intersectObjects(picks, false)[0]?.object;
      const fl = hit?.userData['floor'];
      return typeof fl === 'number' ? { k: 'floor', floor: fl as FloorId } : null;
    }
    let down: { x: number; y: number } | null = null;
    const onDown = (e: PointerEvent) => {
      down = { x: e.clientX, y: e.clientY };
    };
    const onUp = (e: PointerEvent) => {
      const d = down;
      down = null;
      if (d === null || Math.hypot(e.clientX - d.x, e.clientY - d.y) > CLICK_SLOP) return;
      const hit = pick(e.clientX, e.clientY);
      if (hit !== null) latest.current.onPick(hit);
    };
    let lastHover = '';
    const onMove = (e: PointerEvent) => {
      if (e.buttons !== 0) return;
      const hit = pick(e.clientX, e.clientY);
      const key = hit === null ? '' : JSON.stringify(hit);
      const r = canvas.getBoundingClientRect();
      const floor = hit?.k === 'floor' ? hit.floor : null;
      if (key !== lastHover) {
        lastHover = key;
        S.hoverFloor = floor;
        canvas.style.cursor = hit === null ? 'grab' : 'pointer';
        loop.invalidate();
      }
      latest.current.onHover(hit, e.clientX - r.left, e.clientY - r.top);
    };
    const onLeave = () => {
      lastHover = '';
      S.hoverFloor = null;
      latest.current.onHover(null, 0, 0);
      loop.invalidate();
    };
    const onLost = (e: Event) => {
      e.preventDefault();
      latest.current.onError('webglcontextlost');
    };
    canvas.addEventListener('pointerdown', onDown);
    canvas.addEventListener('pointerup', onUp);
    canvas.addEventListener('pointermove', onMove);
    canvas.addEventListener('pointerleave', onLeave);
    canvas.addEventListener('webglcontextlost', onLost);
    const onControls = () => loop.invalidate();
    controls.addEventListener('change', onControls);
    const onVisible = () => {
      if (!document.hidden) loop.invalidate();
    };
    document.addEventListener('visibilitychange', onVisible);

    // --- khởi tạo ---
    world.setTime(S.model.timeMode);
    loop.setMotion(S.motion);
    resize();
    ensureInterior(5);
    if (typeof S.model.view === 'number') ensureInterior(S.model.view);
    syncData();
    applyState();
    secretaryTween();
    loop.invalidate();

    return () => {
      disposed = true;
      api.current = null;
      loop.dispose();
      controls.removeEventListener('change', onControls);
      controls.dispose();
      document.removeEventListener('visibilitychange', onVisible);
      canvas.removeEventListener('pointerdown', onDown);
      canvas.removeEventListener('pointerup', onUp);
      canvas.removeEventListener('pointermove', onMove);
      canvas.removeEventListener('pointerleave', onLeave);
      canvas.removeEventListener('webglcontextlost', onLost);
      ro.disconnect();
      for (const it of S.interiors.values()) it.dispose();
      S.interiors.clear();
      figures.dispose();
      glow.dispose();
      reg.disposeAll();
      renderer.dispose();
      renderer.forceContextLoss();
      canvas.remove();
    };
    } catch (e) {
      // Lỗi dựng sau khi đã có renderer: giải phóng context rồi báo để vỏ lùi về cảnh 2D.
      reg.disposeAll();
      renderer.dispose();
      renderer.forceContextLoss();
      canvas.remove();
      latest.current.onError(e instanceof Error ? e.message : 'Không dựng được cảnh 3D');
      return undefined;
    }
  }, []);

  useEffect(() => {
    api.current?.apply(props.model);
  }, [props.model]);
  useEffect(() => {
    api.current?.motion(props.motion);
  }, [props.motion]);

  return <div ref={host} className="building__host" style={{ position: 'absolute', inset: 0 }} />;
}
