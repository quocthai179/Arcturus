// Cảnh 3D của pet (docs/PLAN.md §7.3). Nạp muộn bằng `React.lazy` nên three.js và three-vrm không vào bundle của popup.
//
// Vòng vẽ theo nhu cầu: chỉ chạy trong một khoảng ngắn sau mỗi thay đổi (đổi tâm trạng, vuốt ve, đang nói, con trỏ đến gần),
// tối đa 30 FPS, rồi chờ tóc/tay áo (spring bone) lắng và dừng hẳn. Trang ẩn hoặc giảm chuyển động: đặt tư thế cuối, vẽ một
// khung, không chạy vòng nào. Gỡ component thì giải phóng toàn bộ GPU (geometry, texture, WebGL context).
import { VRMLoaderPlugin, VRMUtils, type VRM } from '@pixiv/three-vrm';
import { useEffect, useRef } from 'react';
import {
  AmbientLight,
  Clock,
  DirectionalLight,
  Object3D,
  PerspectiveCamera,
  Quaternion,
  Scene,
  SRGBColorSpace,
  Vector3,
  WebGLRenderer,
} from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import type { PetMood } from '../types';
import { EXPRESSIONS, MOOD_POSES, type ExpressionName } from './moods';
import { armQuats, headQuat, measureRig, type Rig } from './pose';

export interface PetSceneProps {
  model: ArrayBuffer;
  mood: PetMood;
  speaking: boolean;
  /** Được phép chuyển động (trang hiện, không bật giảm chuyển động). */
  motion: boolean;
  /** Vị trí con trỏ trong khung canvas (−1…1), `null` khi ở xa. */
  pointer: { x: number; y: number } | null;
  /** Tăng mỗi lần vuốt ve (phản ứng ngắn). */
  reaction: number;
  onReady: () => void;
  onError: (message: string) => void;
}

/** Sau mỗi thay đổi, vòng vẽ chạy tối đa chừng này rồi dừng (không vòng lặp vô hạn khi rảnh). */
const ACTIVE_MS = 12_000;
/** Chờ spring bone lắng trước khi dừng. */
const SETTLE_MS = 2_000;
const FRAME_MS = 1000 / 30;
const POSED = ['leftUpperArm', 'leftLowerArm', 'rightUpperArm', 'rightLowerArm', 'neck'] as const;

interface Live {
  props: PetSceneProps;
  wake: (ms?: number) => void;
}

function targetsFor(rig: Rig, mood: PetMood, waveOffset: number): Record<(typeof POSED)[number], Quaternion> {
  const pose = MOOD_POSES[mood];
  const L = armQuats(rig, 'L', pose.wave === 'L' ? { ...pose.arms.L, elbow: pose.arms.L.elbow + waveOffset } : pose.arms.L);
  const R = armQuats(rig, 'R', pose.wave === 'R' ? { ...pose.arms.R, elbow: pose.arms.R.elbow + waveOffset } : pose.arms.R);
  return {
    leftUpperArm: L.upper,
    leftLowerArm: L.lower,
    rightUpperArm: R.upper,
    rightLowerArm: R.lower,
    neck: headQuat(rig, pose.head),
  };
}

function rigOf(vrm: VRM): Rig {
  vrm.scene.updateMatrixWorld(true);
  const p = (name: 'leftUpperArm' | 'rightUpperArm' | 'leftLowerArm' | 'rightLowerArm') => {
    const node = vrm.humanoid.getNormalizedBoneNode(name);
    if (node === null) throw new Error(`Model thiếu xương ${name}`);
    return vrm.scene.worldToLocal(node.getWorldPosition(new Vector3()));
  };
  return measureRig({
    leftUpperArm: p('leftUpperArm'),
    rightUpperArm: p('rightUpperArm'),
    leftLowerArm: p('leftLowerArm'),
    rightLowerArm: p('rightLowerArm'),
  });
}

export default function PetScene(props: PetSceneProps) {
  const host = useRef<HTMLDivElement>(null);
  const live = useRef<Live>({ props, wake: () => undefined });
  live.current.props = props;

  // Dựng cảnh một lần cho mỗi model.
  useEffect(() => {
    const el = host.current;
    if (el === null) return undefined;
    let disposed = false;
    let frame = 0;
    let vrm: VRM | null = null;
    let rig: Rig | null = null;
    const renderer = new WebGLRenderer({ alpha: true, antialias: true, powerPreference: 'low-power' });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setClearColor(0x000000, 0);
    renderer.outputColorSpace = SRGBColorSpace;
    el.appendChild(renderer.domElement);
    const scene = new Scene();
    const camera = new PerspectiveCamera(24, 1, 0.05, 30);
    const light = new DirectionalLight(0xffffff, Math.PI);
    light.position.set(1, 2, 3);
    scene.add(light, new AmbientLight(0xffffff, 0.6));
    const look = new Object3D();
    camera.add(look);
    scene.add(camera);
    const size = () => {
      const r = el.getBoundingClientRect();
      renderer.setSize(Math.max(1, r.width), Math.max(1, r.height), false);
      camera.aspect = Math.max(1, r.width) / Math.max(1, r.height);
      camera.updateProjectionMatrix();
    };
    size();

    const clock = new Clock(false);
    let running = false;
    let last = 0;
    let activeUntil = 0;
    let settleFrom = 0;
    let blinkAt = 0;
    let blinkT = -1;
    let reacted = props.reaction;
    let reactUntil = 0;
    const expr: Partial<Record<ExpressionName | 'aa', number>> = {};

    const aim = () => {
      const ptr = live.current.props.pointer;
      look.position.set(ptr === null ? 0 : ptr.x * 1.5, ptr === null ? 0 : ptr.y * 1.2 + 0.2, -3);
    };

    const setExpr = (name: ExpressionName | 'aa', v: number) => {
      expr[name] = v;
      vrm?.expressionManager?.setValue(name, v);
    };

    const applyInstant = () => {
      if (vrm === null || rig === null) return;
      const { mood } = live.current.props;
      const t = targetsFor(rig, mood, 0);
      for (const name of POSED) vrm.humanoid.getNormalizedBoneNode(name)?.quaternion.copy(t[name]);
      for (const x of EXPRESSIONS) setExpr(x, MOOD_POSES[mood].expr[x] ?? 0);
      setExpr('aa', 0);
      aim();
      for (let i = 0; i < 60; i++) vrm.update(1 / 30);
      renderer.render(scene, camera);
    };

    const step = (ts: number) => {
      frame = 0;
      if (disposed || vrm === null || rig === null) return;
      const p = live.current.props;
      if (!p.motion || document.hidden) {
        running = false;
        applyInstant();
        return;
      }
      if (ts - last < FRAME_MS) {
        frame = requestAnimationFrame(step);
        return;
      }
      last = ts;
      const dt = Math.min(clock.getDelta(), 0.1);
      const t = clock.elapsedTime;
      const now = performance.now();
      const active = now < activeUntil || p.speaking;
      const pose = MOOD_POSES[p.mood];
      const wave = active && pose.wave !== undefined ? Math.sin(t * 7) * 16 : 0;
      const targets = targetsFor(rig, p.mood, wave);
      const k = 1 - Math.exp(-dt * 9);
      for (const name of POSED) vrm.humanoid.getNormalizedBoneNode(name)?.quaternion.slerp(targets[name], k);

      const happyBoost = now < reactUntil ? 0.7 : 0;
      for (const x of EXPRESSIONS) {
        if (x === 'blink' && p.mood !== 'sleep' && blinkT >= 0) continue;
        const want = x === 'happy' ? Math.max(pose.expr.happy ?? 0, happyBoost) : (pose.expr[x] ?? 0);
        const cur = expr[x] ?? 0;
        setExpr(x, cur + (want - cur) * k);
      }
      // Khẩu hình khi đang đọc lời (P9): mở/khép miệng theo nhịp, không cần phân tích âm.
      setExpr('aa', p.speaking ? 0.25 + 0.35 * Math.abs(Math.sin(t * 11)) : (expr.aa ?? 0) * (1 - k));
      if (active && p.mood !== 'sleep') {
        vrm.humanoid.getNormalizedBoneNode('spine')?.quaternion.setFromAxisAngle(rig.lateralL, Math.sin(t * 1.6) * 0.012);
        if (blinkT < 0 && t > blinkAt) blinkT = 0;
      }
      if (blinkT >= 0) {
        blinkT += dt;
        setExpr('blink', blinkT < 0.08 ? blinkT / 0.08 : Math.max(0, 1 - (blinkT - 0.08) / 0.1));
        if (blinkT > 0.18) {
          blinkT = -1;
          blinkAt = t + 2.5 + Math.random() * 3;
          setExpr('blink', p.mood === 'sleep' ? 1 : 0);
        }
      }
      aim();
      vrm.update(dt);
      renderer.render(scene, camera);

      if (active) {
        settleFrom = now;
      } else if (now - settleFrom > SETTLE_MS) {
        running = false;
        clock.stop();
        return;
      }
      frame = requestAnimationFrame(step);
    };

    const wake = (ms = ACTIVE_MS) => {
      activeUntil = Math.max(activeUntil, performance.now() + ms);
      if (running || disposed || vrm === null) return;
      if (!live.current.props.motion || document.hidden) {
        applyInstant();
        return;
      }
      running = true;
      clock.start();
      clock.getDelta();
      last = 0;
      frame = requestAnimationFrame(step);
    };
    live.current.wake = (ms) => {
      const p = live.current.props;
      if (p.reaction !== reacted) {
        reacted = p.reaction;
        reactUntil = performance.now() + 1500;
      }
      wake(ms);
    };
    const onVisible = () => {
      if (!document.hidden) wake(1500);
    };
    document.addEventListener('visibilitychange', onVisible);
    const onResize = () => {
      size();
      wake(500);
    };
    window.addEventListener('resize', onResize);

    const loader = new GLTFLoader();
    loader.register((parser) => new VRMLoaderPlugin(parser));
    loader.parse(
      props.model,
      '',
      (gltf) => {
        if (disposed) {
          VRMUtils.deepDispose(gltf.scene);
          return;
        }
        const v = gltf.userData.vrm as VRM | undefined;
        if (v === undefined) {
          live.current.props.onError('File không chứa model VRM');
          return;
        }
        try {
          VRMUtils.removeUnnecessaryVertices(gltf.scene);
          VRMUtils.combineSkeletons(gltf.scene);
          VRMUtils.rotateVRM0(v);
          v.scene.traverse((o) => {
            o.frustumCulled = false;
          });
          v.scene.rotation.y += Math.PI * 0.06;
          if (v.lookAt != null) v.lookAt.target = look;
          scene.add(v.scene);
          rig = rigOf(v);
          vrm = v;
          const head = v.humanoid.getNormalizedBoneNode('head');
          const headY = head === null ? 1.4 : v.scene.worldToLocal(head.getWorldPosition(new Vector3())).y;
          const H = headY * 1.08;
          camera.position.set(0, H * 0.72, H * 2.05);
          camera.lookAt(0, H * 0.66, 0);
          applyInstant();
          live.current.props.onReady();
          wake(3000);
        } catch (e) {
          live.current.props.onError(e instanceof Error ? e.message : 'Không dựng được model');
        }
      },
      (err) => live.current.props.onError(err instanceof Error ? err.message : 'File model hỏng'),
    );

    return () => {
      disposed = true;
      if (frame !== 0) cancelAnimationFrame(frame);
      document.removeEventListener('visibilitychange', onVisible);
      window.removeEventListener('resize', onResize);
      if (vrm !== null) VRMUtils.deepDispose(vrm.scene);
      renderer.dispose();
      renderer.forceContextLoss();
      renderer.domElement.remove();
    };
    // Dựng lại chỉ khi đổi model; các props khác đọc qua `live`.
  }, [props.model]);

  // Đổi tâm trạng, vuốt ve, bắt đầu nói, bật lại chuyển động: chạy vòng vẽ một lúc.
  useEffect(() => {
    live.current.wake();
  }, [props.mood, props.reaction, props.speaking, props.motion]);

  // Con trỏ: chỉ nhìn theo khi ở gần, kéo dài vòng vẽ một chút.
  const px = props.pointer?.x ?? null;
  const py = props.pointer?.y ?? null;
  useEffect(() => {
    live.current.wake(px === null ? 600 : 1500);
  }, [px, py]);

  return <div ref={host} className="pet__scene" aria-hidden="true" data-pointer={px === null ? 'far' : 'near'} />;
}
