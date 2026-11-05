# Văn phòng 3D "Tòa nhà Arcturus" (cảnh `building`) — thiết kế

Quyết định: `docs/adr/0003-office-building.md` (đề xuất, người dùng đã duyệt mockup 2026-11-05; thay đổi cùng ngày: thư ký làm
việc ở Phòng CEO trên cùng). Chưa có mã ARC: đề xuất ticket dưới story T13 · ARC-34, làn `ui` (§15).

Mockup đã duyệt (tham chiếu hình ảnh, three r128, dữ liệu mẫu): `toa-nha-arcturus-3d.html` + ảnh `ext-3d.png`, `f1-3d.png` trong
scratchpad của phiên thiết kế. Các hằng số cần thiết đã chép vào tài liệu này; tài liệu tự đủ để làm, không cần mở mockup.

## 0. Phạm vi

- Chỉ `ui/` (làn `ui`). Không đổi hub, `schema.rs`, `protocol.rs`, `docs/PROTOCOL.md`, `crates/app/`, CSP (WebGL và
  `CanvasTexture` không cần `blob:`).
- Cảnh mới `building` cắm qua `OFFICE_SCENES` (`ui/src/components/scenes.tsx`). Mặc định vẫn là `live` tới khi đo ngân sách (§9.5).
- Hàng đợi duyệt (`QueueList` trong `aside.office__queue` của `ui/src/views/Office.tsx`) không đổi, luôn ở ngoài cảnh.
- Không làm: âm thanh, giọng nói trong Office, VRM trong tòa nhà (ADR 0001 điểm 9), giao việc bằng LLM (tầng 2, ADR 0002),
  đồng bộ thư ký giữa cửa sổ Office và cửa sổ Pet.

## 1. Mô hình tòa nhà

Tòa nhà = máy này. Mỗi tầng 0–4 = một nhóm agent; tầng 5 là Phòng CEO cố định. Số tầng là hằng, **không bao giờ** sinh thêm từ dữ liệu.

| Tầng | Khóa | Tên | Nguồn agent | Vai trò cố định (thứ tự = thứ tự bàn) |
|---|---|---|---|---|
| 0 | `lobby` | Sảnh | phiên chính (`kind = session`), `general-purpose`, agent không thuộc plugin nào, khu **khách** (plugin lạ, chuỗi sai dạng) | — (khu `session` 6 bàn, `general` 4 bàn, `guest` 4 chỗ sofa) |
| 1 | `dev` | Dev | plugin `dev-squad` | orchestrator, business-analyst, project-manager, architect, developer, qa-engineer, code-reviewer, debugger, devops-engineer, security-auditor, performance-engineer, database-engineer, sre-incident-responder, dependency-upgrader, tech-writer (15) |
| 2 | `test` | Test | plugin `tester-squad` | test-lead, test-analyst, test-designer, exploratory-tester, functional-tester, api-tester, ux-accessibility-tester, nonfunctional-tester, automation-engineer, bug-triager, test-reporter (11) |
| 3 | `rnd` | R&D | plugin `lab` | scout, reader, ideator, experimenter, analyst, skeptic, writer (7) |
| 4 | `media` | Media | plugin `demo-builder` | demo-director, demo-strategist, scriptwriter, flow-engineer, screen-recorder, voice-producer, video-editor, localizer, demo-qa, launch-copywriter (10) |
| 5 | `ceo` | Phòng CEO | **không nhận agent nào** | bàn CEO (luôn trống: người dùng là CEO), bàn chỉ huy của thư ký |

Màu nhấn (TS cho three; CSS dùng token mới trong `:root` của `index.css`: `--floor-lobby`… vì `styles.test.ts` cấm hex ngoài `:root`):
lobby `#9AA3B5`, dev `#8B7CF6`, test `#34D1B5`, rnd `#FFD27A`, media `#FF7A59`, ceo `#FFB020`.

Nhãn ngắn trên bàn (danh sách trắng, chép từ mockup): business-analyst→`ba`, project-manager→`pm`, qa-engineer→`qa`,
code-reviewer→`reviewer`, devops-engineer→`devops`, security-auditor→`security`, performance-engineer→`perf`,
database-engineer→`database`, sre-incident-responder→`sre`, dependency-upgrader→`upgrader`, test-analyst→`analyst`,
test-designer→`designer`, exploratory-tester→`exploratory`, functional-tester→`functional`, api-tester→`api`,
ux-accessibility-tester→`ux-a11y`, nonfunctional-tester→`non-func`, automation-engineer→`automation`, bug-triager→`triager`,
test-reporter→`reporter`, demo-director→`director`, demo-strategist→`strategist`, flow-engineer→`flow`,
screen-recorder→`recorder`, voice-producer→`voice`, video-editor→`editor`, launch-copywriter→`copywriter`; còn lại giữ nguyên.

## 2. Dữ liệu vào cảnh

### 2.1 `OfficeSceneProps` (đổi `ui/src/components/DeskGridScene.tsx`, tương thích ngược)

```ts
export interface OfficeSceneProps {
  desks: readonly AgentView[];
  support: readonly AgentView[];
  canSelect: (agent: AgentView) => boolean;
  onSelect: (agentId: string) => void;
  motion?: boolean;
  /** Tóm tắt chỉ đọc cho cảnh (thư ký, số liệu tầng). Tùy chọn: `live`, `grid` bỏ qua. Không chứa lệnh. */
  brief?: SceneBrief;
}
```

Cảnh `building` gộp `[...desks, ...support]` (phân nhóm "phòng hỗ trợ" không áp dụng cho tòa nhà; mỗi agent có chỗ trong tầng của nó).

### 2.2 `SceneBrief` (`ui/src/sceneBrief.ts`, thuần, bundle chính, không import `three`)

```ts
import type { AgentId, Risk } from './protocol';
import type { HubState } from './store';
import type { BrainOutput, SpeechPriority } from './pet/brain';
import type { PetMood } from './pet/types';

export interface PendingBrief {
  count: number;      // số yêu cầu đang chờ của agent
  risk: Risk;         // rủi ro cao nhất
  tool: string;       // tool của yêu cầu cũ nhất (dữ liệu không tin cậy, chỉ hiện qua cleanText)
  oldestAt: number;   // Date.parse(created_at) của yêu cầu cũ nhất
}

export interface SceneBrief {
  connected: boolean;           // connection === 'open' && synced
  paused: boolean;
  focus: boolean;
  pending: ReadonlyMap<AgentId, PendingBrief>;
  handoffs: ReadonlySet<AgentId>;   // agent có thẻ "Duyệt trong terminal"
  speech: { key: string; text: string; priority: SpeechPriority } | null;  // từ think()
  teamMood: PetMood;                // từ think(); 'sleep' = đội rảnh lâu
}

export function sceneBrief(state: HubState, out: Pick<BrainOutput, 'speech' | 'mood'>): SceneBrief;
export const RISK_RANK: Record<Risk, number>; // critical 0, high 1, medium 2, low 3
```

`ui/src/useSceneBrief.ts`:

```ts
export function useSceneBrief(state: HubState): SceneBrief;
```

Giữ `BrainMemory` trong `useRef(initialMemory(Date.now()))`; trong `useEffect([state])` gọi
`think(state, mem, Date.now(), SLEEP_AFTER_MS, 'normal')`, lưu `out.memory`, `setBrief(sceneBrief(state, out))`.
**Không gọi `think` trong `useMemo`/render** (StrictMode gọi hai lần → lần hai thấy khóa đã nói, mất câu). Không timer nào.
`Office` gọi hook này và truyền `brief` cho mọi cảnh. `brief` không có `command_original`/`command_normalized` (quy tắc 3).

## 3. Xếp tầng (`ui/src/building/floors.ts`)

### 3.1 Kiểu và hằng

```ts
export type FloorId = 0 | 1 | 2 | 3 | 4 | 5;
export type AgentFloorId = 0 | 1 | 2 | 3 | 4;          // tầng 5 không có trong kiểu trả về của placeAgent
export const CEO_FLOOR = 5 as const;
export const FLOOR_COUNT = 6;
export type FloorKey = 'lobby' | 'dev' | 'test' | 'rnd' | 'media' | 'ceo';
export interface RoleDef { id: string; short: string }
export interface FloorDef { id: FloorId; key: FloorKey; name: string; plugin: string | null; accent: string; blurb: string; roles: readonly RoleDef[] }
export const FLOORS: readonly FloorDef[];                       // 6 mục, bảng §1
export const PLUGIN_FLOORS: ReadonlyMap<string, AgentFloorId>;  // dev-squad→1, tester-squad→2, lab→3, demo-builder→4
export const ROLE_MAX = 128;                                    // dài hơn → khách
export const SEGMENT = /^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/;
export const HUB_DEFAULT_SUBAGENT_ROLE = 'trợ lý';             // khớp DEFAULT_SUBAGENT_ROLE ở crates/hub-core/src/persona.rs
export const MATCH_BARE_ROLE = true;                           // §3.2 bước 5, xem câu hỏi mở Q2

export type Zone = 'session' | 'general' | 'guest' | 'role' | 'extra';
export interface Placement { floor: AgentFloorId; zone: Zone; roleId: string | null }  // roleId luôn là id trong danh sách trắng

export function parseAgentType(raw: string): { plugin: string | null; name: string } | null;
export function placeAgent(a: Pick<AgentView, 'kind' | 'role'>): Placement;
```

### 3.2 Thuật toán `placeAgent`

1. `kind === 'session'` → `{ floor: 0, zone: 'session', roleId: null }` (role của phiên chính là persona, không phải `agent_type`).
2. `role === ''` hoặc `role === HUB_DEFAULT_SUBAGENT_ROLE` → `{ 0, 'general', null }`.
3. `parseAgentType(role)`: độ dài (code unit) > `ROLE_MAX` → `null`; tách theo `:`; 1 đoạn → `{ plugin: null, name }`; 2 đoạn →
   `{ plugin, name }`; nhiều hơn 2 → `null`; mỗi đoạn phải khớp `SEGMENT` (không dấu cách, không `/`, `.`, `\`, ký tự ẩn, chữ có dấu),
   sai → `null`. Không chuẩn hóa Unicode, không đổi hoa thường. `null` → `{ 0, 'guest', null }`.
4. Có `plugin`: tra `PLUGIN_FLOORS.get(plugin)` (Map: `__proto__`, `constructor` không trúng gì). Không có → `{ 0, 'guest', null }`.
   Có tầng `f`: `name` là id vai trò của `FLOORS[f].roles` → `{ f, 'role', name }`, không → `{ f, 'extra', null }`.
5. Không có `plugin`: `name === 'general-purpose'` → `{ 0, 'general', null }`. Nếu `MATCH_BARE_ROLE` và `name` trùng **chính xác**
   id vai trò của đúng một tầng 1–4 (các id không trùng nhau giữa các tầng) → `{ f, 'role', name }`. Còn lại → `{ 0, 'general', null }`
   (agent riêng của người dùng như `Explore`, `Plan`, `my-helper`).
6. `kind === 'teammate'` đi như subagent (bước 2–5); agent teams chưa kiểm chứng (Q9 HOOK_FINDINGS).

Không nhánh nào trả tầng 5. Kết quả chỉ dùng để chọn chỗ vẽ; không ảnh hưởng duyệt, policy, `canSelect`.

### 3.3 Ví dụ (test bảng, `floors.test.ts`)

| `kind` | `role` | Kết quả |
|---|---|---|
| subagent | `dev-squad:developer` | 1 · role `developer` |
| subagent | `dev-squad:architect` | 1 · role `architect` |
| subagent | `dev-squad:new-role` | 1 · extra (bàn linh hoạt) |
| subagent | `tester-squad:api-tester` | 2 · role `api-tester` |
| subagent | `lab:analyst` | 3 · role `analyst` |
| subagent | `general-purpose` | 0 · general |
| subagent | `Explore` | 0 · general |
| subagent | `developer` (trần) | 1 · role `developer` (khi `MATCH_BARE_ROLE`) |
| subagent | `evil:analyst` | 0 · guest (plugin lạ không "mượn" tầng R&D) |
| subagent | `evil:../x` | 0 · guest (`/`, `.` sai dạng) |
| subagent | `ceo:secretary`, `ceo`, `5` | 0 · guest / 0 · general / 0 · general (không bao giờ tầng 5) |
| subagent | `DEV-SQUAD:developer` | 0 · guest (so chính xác) |
| subagent | `dev-squad:developer​` | 0 · guest |
| subagent | `__proto__:x`, `constructor` | 0 · guest / 0 · general |
| subagent | `a:b:c` | 0 · guest |
| subagent | `x` lặp 200 lần | 0 · guest (quá `ROLE_MAX`) |
| subagent | `` hoặc `trợ lý` | 0 · general |
| session | `backend` (persona) | 0 · session |
| teammate | `lab:scout` | 3 · role `scout` |

## 4. Bố cục (`ui/src/building/layout.ts`, thuần)

Đơn vị mét, trục y lên, mặt trước tòa nhà hướng +z. Kiểu điểm `{ x: number; y: number; z: number }` (`Vec3` riêng, không dùng
`THREE.Vector3`). Mọi tọa độ tầng là **cục bộ** so với tâm tầng `(cx, BASE[n], cz)`.

### 4.1 Khối tầng

```ts
export interface FloorGeo { w: number; d: number; cx: number; cz: number; H: number }
export const GEO: readonly FloorGeo[] = [
  { w: 24, d: 16,   cx: 0,  cz: 0,     H: 6.0 },  // 0 Sảnh (cao, có mái đón trước cửa)
  { w: 22, d: 15,   cx: 0,  cz: 0,     H: 3.8 },  // 1 Dev
  { w: 24, d: 17,   cx: 0,  cz: 0.75,  H: 3.8 },  // 2 Test (đua ra trước; sân hiên trên mái)
  { w: 22, d: 11,   cx: 0,  cz: -2.5,  H: 3.8 },  // 3 R&D (lùi vào)
  { w: 20, d: 11.5, cx: -1, cz: -2.25, H: 3.8 },  // 4 Media
  { w: 16, d: 10,   cx: -1, cz: -3,    H: 4.4 },  // 5 Phòng CEO (nhỏ hơn, trần cao, kính suốt tầng)
];
export const BASE: readonly number[];   // BASE[0] = 0, BASE[i] = BASE[i-1] + GEO[i-1].H
export const ROOF_Y: number;            // BASE[5] + GEO[5].H — mái + biển ARCTURUS nằm trên Phòng CEO
export const LIFT = 16;                 // các tầng trên tầng đang xem nâng lên chừng này
export const SHAFT_X = 13.8;            // trục thang máy kính (ngoài tòa, bên phải), cạnh 3.2
export const CAR = { w: 2.5, h: 2.7 };
export const FLOOR_SLAB = 0.3;          // sàn đứng ở y = 0.3 cục bộ
```

Mỗi tầng có cầu nối từ mép phải (`x = w/2`) sang trục thang máy ở `z = -cz` (tức z thế giới 0). `ELEVATOR_DOOR[n]` = điểm đứng
trên cầu, `x = w/2 - 0.6`, `z = -cz`.

### 4.2 Lưới bàn tầng 0–4

```ts
export interface DeskGrid { cols: number; x0: number; z0: number; dx: number; dz: number; lounge: [number, number] }
export const LAY: readonly DeskGrid[] = [
  { cols: 5, x0: -10.5, z0: -5.2, dx: 2.1, dz: 2.8, lounge: [-8.5, 3.2] },
  { cols: 5, x0: -9.2,  z0: -5.0, dx: 2.1, dz: 2.7, lounge: [6.0, 4.6] },
  { cols: 5, x0: -10.2, z0: -6.0, dx: 2.1, dz: 2.7, lounge: [6.2, 5.0] },
  { cols: 4, x0: -9.0,  z0: -3.4, dx: 2.1, dz: 2.7, lounge: [5.2, 2.6] },
  { cols: 5, x0: -8.2,  z0: -3.2, dx: 2.1, dz: 2.7, lounge: [-7.4, 3.6] },
];
export const HOT_DESKS = 4;
export function deskAt(floor: AgentFloorId, index: number): { desk: Vec3; seat: Vec3 };
// desk = (x0 + (i % cols)·dx, 0.3, z0 + ⌊i / cols⌋·dz); seat = desk + (0, 0, 0.95); nhân vật quay mặt về −z (màn hình).
```

- Tầng 1–4: chỉ số bàn `0 … roles.length-1` là bàn vai trò, `roles.length … roles.length+3` là 4 bàn linh hoạt (cùng lưới, hàng
  kế tiếp). Đã kiểm vừa sàn: Dev 19 bàn (4 hàng), Test 15, R&D 11, Media 14.
- Tầng 0: bàn `0–5` khu `session`, `6–9` khu `general`; khu `guest` là 4 chỗ sofa ở góc nghỉ `lounge`.
- `LOUNGE_SPOTS[n]` (6 chỗ): 3 chỗ ngồi sofa `(ax, ax+1, ax+2; az-1.7)`, 3 chỗ đứng quanh bàn trà `(ax+1±1.1, az+0.9)` và `(ax+1, az+1.3)`.
- `STANDING_SPOTS[n]` (4 chỗ): hàng `(ax-1 … ax+2, az+2.2)`, cho agent không còn bàn.
- `briefSpot(seat)` = `seat + (0.75, 0, 0.15)`: chỗ thư ký đứng cạnh agent. Lối đi: cột `x = seat.x + 1.05` (khe giữa hai bàn rộng 0.6 m)
  và hàng ngang `aisleZ[n] = z0 + rows·dz - 0.4`.

### 4.3 Nội thất (chi tiết hơn mockup; builder trong `three/interiors.ts`, dữ liệu vị trí trong `layout.ts`)

Mỗi tầng: bản sàn sáng + viền màu nhấn, thảm dưới khối bàn (màu nhấn 22 %), tường sau và trái đặc có dải đèn nhấn, kính mặt
dựng + nan nhôm (InstancedMesh), trần có tấm đèn, biển số tầng, cây, góc nghỉ (sofa, bàn trà, biển "GÓC NGHỈ").

Bàn làm việc (InstancedMesh, một mesh mỗi phần, chi tiết hơn mockup): mặt bàn 1.5×0.05×0.7 + 2 chân khung, **2 màn hình** (chính
0.62×0.38 sáng theo mood, phụ 0.4×0.3 xoay 20°), chân màn, bàn phím 0.45×0.02×0.15, chuột, cốc (trụ 0.04), ghế: đệm, tựa, trụ,
chân sao 5 nhánh (một mesh gộp). Màu màn hình theo trạng thái người ngồi: working `#8B7CF6`, approval `#FFB020`, error `#FF6B5A`,
done `#34D1B5`, idle ×0.55, trống `#12141A`.

| Tầng | Đạo cụ |
|---|---|
| 0 Sảnh | quầy lễ tân bình thường (đá sáng 5.2×1.1×1.3, mặt tối, dải sáng; **không có Pet**), tường LED chữ ARCTURUS, 2 bộ sofa chờ (khu khách), bàn trà, đèn thả 3 dải, cửa kính chính + mái đón, giá báo, 3 chậu cây lớn, cửa thang máy trong sảnh |
| 1 Dev | bảng kanban Todo/Doing/Done (texture), bàn họp tròn 6 ghế, kệ sách, quầy cà phê (máy pha, cốc), đèn báo build (đỏ/xanh), tủ server nhỏ có đèn LED (InstancedMesh) |
| 2 Test | tường thiết bị (12 điện thoại/máy tính bảng, texture), bảng bug màu theo mức, bàn thử có thảm "PASS", kệ thiết bị, sân hiên trên mái (ván gỗ, chậu, ghế dài, dây đèn 15 bóng) |
| 3 R&D | kệ sách cao (texture gáy sách), bảng trắng có biểu đồ, tủ GPU có đèn, 2 ghế bành đọc sách, đèn cây |
| 4 Media | phòng thu kính có micro và biển ON AIR, phông xanh, máy quay trên chân ba, 2 đèn softbox, bàn dựng 3 màn hình |
| 5 Phòng CEO | §4.4 |

### 4.4 Phòng CEO (tầng 5)

Bố cục cục bộ (`GEO[5]`: rộng 16, sâu 10; mặt trước và mặt phải là kính suốt từ sàn tới trần, nhìn ra thành phố):

```
          tường trạng thái: 5 màn hình (tầng 0…4)            z = −4.74
   ┌──────────────────────────────────────────────────────────┐
   │ [0 Sảnh] [1 Dev] [2 Test] [3 R&D] [4 Media]              │
   │                                                          │
   │ kệ giải thưởng   ┌─BÀN CEO (trống)─┐  ┌─BÀN CHỈ HUY─┐     │ cầu nối →
   │                  │ ghế CEO trống   │  │ 3 màn hình  │     │ thang máy
   │                  └─────────────────┘  │ THƯ KÝ ngồi │     │
   │  sofa tiếp khách + bàn trà            └─────────────┘     │
   │  thảm lớn, 2 cây, đèn cây                                │
   └──────────── kính suốt (nhìn ra thành phố) ───────────────┘ z = +5
```

```ts
export const CEO_DESK = { at: { x: -1.5, y: 0.3, z: -1.8 }, w: 2.6, d: 0.9 };         // bàn gỗ tối chữ L, ghế CEO cao lưng, luôn trống
export const SECRETARY_DESK = { at: { x: 3.4, y: 0.3, z: -1.2 }, w: 1.8, d: 0.8 };   // bàn chỉ huy, 3 màn hình cong, tai nghe trên giá
export const SECRETARY_HOME: Vec3 = { x: 3.4, y: 0.3, z: -0.3 };                     // chỗ ngồi của thư ký
export const STATUS_WALL = { z: -4.74, y: 2.4, xs: [-6, -3.6, -1.2, 1.2, 3.6], w: 2.1, h: 1.2 }; // màn hình tầng 0…4
export const CEO_LOUNGE: Vec3 = { x: -5.2, y: 0.3, z: 2.4 };                          // sofa tiếp khách + bàn trà
```

- **Tường trạng thái**: mỗi tầng một màn hình (`CanvasTexture` 256×144): số tầng, tên, `▶ <đang làm>`, `! <giơ tay>`, viền màu nhấn;
  viền cam khi có người giơ tay, đỏ khi có Critical. Chỉ vẽ lại texture khi số đổi (so chuỗi nội dung), không theo giờ.
- **Bàn CEO trống**: ghế xoay nhẹ về phía cửa kính; đèn bàn; biển tên "CEO" (chữ cố định, không lấy tên người dùng).
- **Nhìn ra thành phố**: kính mặt trước/phải trong suốt hơn tầng khác (opacity 0.08); khối nhà hàng xóm và đèn đường của khu đất
  (§9.2) thấy qua kính; trời theo giờ máy.
- Biển ARCTURUS và mái (ván gỗ, vài chậu cây) nằm trên `ROOF_Y`, ngay trên Phòng CEO; xem Phòng CEO thì mái được nâng `LIFT`.
- Không có bàn agent, không có hot desk: `placeAgent` không trả tầng 5 nên không agent nào tới đây.

## 5. Xếp chỗ và tư thế (`ui/src/building/seating.ts`, thuần)

```ts
export type SpotKind = 'desk' | 'lounge' | 'standing';
export interface Spot { floor: AgentFloorId; kind: SpotKind; index: number }
export interface SeatMap { readonly byAgent: ReadonlyMap<AgentId, Spot>; readonly order: readonly AgentId[] } // order = thứ tự xuất hiện
export const MAX_FIGURES_PER_FLOOR = 24;
export const IDLE_SLEEP_MS = 10 * 60_000;  // = SLEEP_AFTER_MS của brain

export interface FloorRoster {
  floor: AgentFloorId;
  figures: readonly { agent: AgentView; placement: Placement; spot: Spot; pose: PoseId }[];
  overflow: number;              // agent không có chỗ vẽ ("+N")
  stats: { present: number; busy: number; raised: number; errors: number };
}

export function assignSeats(prev: SeatMap, agents: readonly AgentView[]): SeatMap;
export function rosters(seats: SeatMap, agents: readonly AgentView[], brief: SceneBrief | undefined, now: number): FloorRoster[]; // 5 mục, tầng 0…4
export function poseFor(a: AgentView, spot: Spot, now: number): PoseId;
```

`assignSeats` (ổn định, giống `live/sim.ts`): agent đã có chỗ hợp lệ thì giữ; agent bị xóa thì trả chỗ; agent mới xét theo `id` tăng
dần. Thứ tự chọn bàn: bàn vai trò của mình (zone `role`) → bàn của khu (Sảnh: `session`/`general`) → bàn linh hoạt → chỗ đứng → tràn.
Hai agent cùng vai trò: người đến trước giữ bàn vai trò, người sau sang bàn linh hoạt. Khách: sofa góc nghỉ Sảnh → chỗ đứng → tràn.

Agent rảnh (`idle`) giữ bàn nhưng được vẽ ở `LOUNGE_SPOTS` nếu còn chỗ (bàn hiện màn hình tối), hết chỗ thì ngồi ở bàn.

| mood | activity | Tư thế (`PoseId`) | Chỗ |
|---|---|---|---|
| working | typing, running | `sit_type` | bàn |
| working | reading | `sit_read` | bàn |
| working | thinking, waiting, idle | `sit_think` | bàn |
| approval | bất kỳ | `raise` (đứng cạnh ghế, tay phải giơ cao) + sprite "!" nếu `canSelect` | bàn |
| error | bất kỳ | `sit_error` | bàn |
| done | bất kỳ | `cheer` | bàn |
| idle | `now − last_event_at < IDLE_SLEEP_MS` | `lounge_sit` (sofa) / `stand` (chỗ đứng) | góc nghỉ |
| idle | ≥ `IDLE_SLEEP_MS` | `sleep` | góc nghỉ hoặc bàn |

Tư thế ngủ không cần timer: được tính lại ở lần dựng kế tiếp (sự kiện mới, đổi tầng, đổi kích thước).

## 6. Nhân vật (`ui/src/building/figure.ts` thuần + `three/figures.ts`)

### 6.1 Hình học (mét, nhân vật đứng cao 1.50, gốc ở chân)

| Phần | Hình | Kích thước | Vị trí đứng (khớp cha) | Màu |
|---|---|---|---|---|
| đầu | cầu | r 0.165, scale (1, 1.04, 0.96) | cổ + 0.19 (y ≈ 1.33) | da `#FFE6D8` |
| tóc chỏm | cầu | r 0.178, scale y 0.8 | đầu + (0, 0.04, 0.03) | `palette.hair` |
| tóc kiểu | `short`: không; `bob`: hộp 0.36×0.22×0.2 sau gáy; `bun`: cầu r 0.07 sau-trên | | đầu | `palette.hair` |
| mắt ×2 | cầu | r 0.022 | đầu + (±0.058, 0.01, −0.15) | mực `#241B4D` |
| miệng | hộp | 0.05×0.012×0.01; scale theo mood (cười rộng, chữ O cao, mếu) | đầu + (0, −0.07, −0.155) | `#B4445E` |
| má ×2 (chỉ `done`, `approval`) | đĩa | r 0.025 | đầu + (±0.09, −0.04, −0.13) | `#FF8FA8` |
| cổ | trụ | r 0.055, h 0.06 | thân + 0.27 | da |
| thân | capsule | r 0.17, dài 0.46, scale (1.05, 1, 0.8) | hông + 0.24 (y ≈ 0.98) | `palette.outfit` |
| thắt lưng | trụ | r 0.172, h 0.05 | y 0.78 | `palette.acc` |
| phù hiệu | hộp | 0.06×0.08×0.01 | ngực trái (−0.08, 1.05, −0.14) | màu nhấn của tầng |
| cánh tay ×2 | capsule | r 0.05, dài 0.26 | vai (±0.2, 1.15, 0) | `palette.outfit` |
| cẳng tay ×2 | capsule | r 0.045, dài 0.24 | khuỷu | `palette.outfit` × 0.85 |
| bàn tay ×2 | cầu | r 0.05 | cổ tay | da |
| đùi ×2 | capsule | r 0.065, dài 0.36 | hông (±0.085, 0.74, 0) | quần `#2C3140` |
| ống chân ×2 | capsule | r 0.058, dài 0.34 | gối | quần |
| giày ×2 | hộp | 0.1×0.07×0.17 | cổ chân, lệch −z 0.03 | `#1A1D25` |

- Kiểu tóc = `fnv1a(agent.id) % 3` → `short | bob | bun` (ổn định, không dựa vào tên).
- Màu: `palette.*` chỉ nhận `/^#[0-9a-fA-F]{6}$/`, sai thì dùng `DEFAULT_PALETTE` (`ui/src/components/Character.tsx`).
- Thư ký: bảng màu cố định `SECRETARY_PALETTE = { hair: '#2E2A5C', outfit: '#8B7CF6', acc: '#FFD166' }`, tóc `bob`, thêm ăng-ten
  (trụ 0.005–0.03 cao 0.4 + cầu vàng r 0.06 trên đỉnh đầu, như mockup), tai nghe (vòng xuyến r 0.17 mảnh), kẹp hồ sơ ở tay trái khi đi.
  Tỉ lệ 1.0 (không thu nhỏ như mockup), nhận diện bằng ăng-ten và vầng sáng vàng dưới chân.

### 6.2 Tư thế (góc độ; dương: vai/hông gập ra trước, khuỷu/gối gập, đầu cúi, thân nghiêng trước)

```ts
export type PoseId = 'stand' | 'walk' | 'sit_type' | 'sit_read' | 'sit_think' | 'sit_error' | 'raise' | 'cheer' | 'lounge_sit' | 'sleep';
export interface Limb { pitch: number; roll: number; bend: number }   // vai/hông: pitch, roll; khuỷu/gối: bend
export interface JointAngles { armL: Limb; armR: Limb; legL: Limb; legR: Limb; head: number; torso: number; pelvisY: number }
export const POSES: Record<Exclude<PoseId, 'walk'>, JointAngles>;
export function walkAngles(phase: number): JointAngles;              // phase 0…1, một chu kỳ 2 bước
export function partTransforms(a: JointAngles, hair: HairStyle, extras: FigureExtras): PartTransform[]; // FK thuần, không three
export interface PartTransform { part: PartId; pos: Vec3; rot: Vec3 /* Euler XYZ radian */; scale: Vec3 }
```

| Tư thế | tay T (pitch/roll/bend) | tay P | chân (pitch/bend) | đầu | thân | pelvisY |
|---|---|---|---|---|---|---|
| stand | 0/8/10 | 0/8/10 | 0/0 | 0 | 0 | 0.74 |
| walk | ±25 ngược pha /6/20 | ∓25 | ∓25 / 10…30 | 0 | 3 | 0.74 + 0.02·|sin| |
| sit_type | 50/10/70 | 50/10/70 | 90/90 | 12 | 5 | 0.45 |
| sit_read | 30/10/100 | 45/10/60 | 90/90 | 18 | 4 | 0.45 |
| sit_think | 40/10/60 | 70/5/140 (tay chống cằm) | 90/90 | 5 | 8 | 0.45 |
| sit_error | 50/10/70 | 150/40/130 (tay ôm đầu) | 90/90 | 15 | 10 | 0.45 |
| raise | 0/8/10 | 170/15/15 | 0/0 | −10 | 0 | 0.74 |
| cheer | 160/30/20 | 160/30/20 | 0/0 | −8 | −3 | 0.74 |
| lounge_sit | 10/12/30 | 10/12/30 | 80/85 | 0 | −12 | 0.42 |
| sleep | 40/10/90 | 40/10/90 | 85/90 | 35 | 20 | 0.45 |

Không có chuyển động thường trực: tư thế là tĩnh. Chỉ `walk` (của thư ký) đổi pha theo thời gian và chỉ trong lúc thư ký đang đi.

### 6.3 Vẽ

- **Tầng đang xem (chi tiết)**: mỗi phần một `InstancedMesh` dùng chung geometry, `MeshLambertMaterial` + `instanceColor`; số
  instance = `MAX_FIGURES_PER_FLOOR + 1` (thư ký). Phần không dùng (tóc kiểu khác, má) đặt scale 0. ≈ 25 draw call cho nhân vật.
- **Ngoại cảnh và các tầng không xem (rút gọn)**: 3 `InstancedMesh` chung cả tòa (thân capsule màu outfit, đầu, tóc chỏm), tư thế
  đứng/ngồi suy từ `pelvisY`; ≤ 5 × 24 + 1 instance.
- Nhãn trên bàn (chỉ tầng đang xem): sprite `CanvasTexture` 256×64: dòng 1 nhãn ngắn vai trò (danh sách trắng) hoặc "khách"/"phiên chính";
  dòng 2 `cleanText(name, 16)` bằng `fillText`. Chờ `document.fonts.ready` trước khi vẽ; thiếu font thì dùng `sans-serif`. Tạo khi vào
  tầng, `dispose()` khi rời tầng. Sprite "!" dùng một texture chung.

## 7. Thư ký trưởng (`ui/src/building/secretary.ts`, `route.ts`, thuần)

### 7.1 Vai trò

Thư ký là danh tính của Pet trong tòa nhà: cùng mẫu câu tầng 0 (`think()` ở `ui/src/pet/brain.ts`), cùng luật giờ tập trung. Nhà của
nó là **bàn chỉ huy ở Phòng CEO** (tầng 5), cạnh bàn CEO trống của người dùng. Nó:

- **được**: theo dõi `brief` + rosters; nói bằng bong bóng (văn bản mẫu); tự đi bộ, đi thang máy xuống tầng cần chú ý, đứng cạnh agent;
  quay về Phòng CEO khi hết việc; tóm tắt cả tòa khi được bấm; đề nghị nút "Xem thẻ duyệt" (gọi `onSelect(agentId)` **khi người dùng
  bấm**) và "Xuống tầng N" / "Về Phòng CEO" (đổi góc nhìn trong cảnh); đánh dấu tầng trong bảng thang máy.
- **không được**: gửi bất kỳ thông điệp nào lên hub (cảnh không có `send`; lệnh đội tầng 1 vẫn ở `TeamControls` phía trên cảnh và cửa
  sổ Pet); duyệt, từ chối, duyệt gộp, mở/đóng batch, trả lời đề xuất tin cậy; tự gọi `onSelect` hay tự kéo focus; hiện lệnh (chỉ `tool` +
  mức rủi ro); gợi ý "nên đồng ý"; giao việc cho agent hay khởi chạy phiên (tầng 2, ADR 0002, chưa duyệt, **ngoài phạm vi**); đọc giọng.

### 7.2 Mục tiêu và ưu tiên

```ts
export type GoalKind = 'request' | 'handoff' | 'error';
export interface Goal { kind: GoalKind; agentId: AgentId; floor: AgentFloorId; key: string; rank: number }
export function pickGoal(rosters: readonly FloorRoster[], brief: SceneBrief | undefined, canSelect: (a: AgentView) => boolean): Goal | null;
```

| rank | Nguồn | Khi giờ tập trung |
|---|---|---|
| 0 | yêu cầu Critical (`brief.pending`) | đi |
| 1 | yêu cầu High | đi |
| 2 | yêu cầu Medium/Low (cũ nhất trước theo `oldestAt`) | không đi (chỉ hiện ở tường trạng thái) |
| 3 | handoff (`brief.handoffs`) | không đi |
| 4 | agent `error` | không đi |

Không có `brief` (cảnh dùng ngoài Office): rank 2 cho agent `approval` có `canSelect`, rank 4 cho `error`. Trùng rank: `oldestAt` rồi `agentId`.
Mất kết nối (`!brief.connected`): không đi đâu, về/ở Phòng CEO.

### 7.3 Máy trạng thái

```ts
export type SecState =
  | { k: 'home' }                                                   // ngồi ở SECRETARY_HOME, tầng 5
  | { k: 'moving'; route: Route; startAt: number; goal: Goal | null } // goal null = đang về Phòng CEO
  | { k: 'briefing'; goal: Goal; since: number; clearedAt: number | null }
  | { k: 'asleep' };                                                // ở home, gục đầu lên bàn chỉ huy

export interface SecMemory { state: SecState; said: readonly string[] /* ≤100 */; bubble: Bubble | null; lastTripAt: number }
export interface Bubble { key: string; text: string; priority: SpeechPriority; until: number; actions: readonly BubbleAction[] }
export type BubbleAction = { kind: 'select'; agentId: AgentId; label: 'Xem thẻ duyệt' } | { kind: 'view'; view: View; label: string };
export interface SecInput { rosters: readonly FloorRoster[]; brief: SceneBrief | undefined; canSelect: (a: AgentView) => boolean; motion: boolean; now: number }

export function initialSecretary(): SecMemory;                      // { state: home }
export function stepSecretary(mem: SecMemory, input: SecInput): { mem: SecMemory; wakeAt: number }; // wakeAt = Infinity khi không cần timer
export function secretaryPose(mem: SecMemory, now: number): { floor: FloorId | 'car'; pos: Vec3; pose: PoseId; facing: number };
export const BRIEF_RETURN_MS = 4_000;   // mục tiêu đã xong thì đứng thêm chừng này rồi đi tiếp/về
export const TRIP_GAP_MS = 3_000;       // không đổi đích nhanh hơn chừng này (trừ khi rank thấp hơn hẳn)
export const SAY_MS = 8_000;            // bong bóng tự ẩn
```

Chuyển trạng thái (gọi `stepSecretary` khi `brief`/rosters đổi và khi tới `wakeAt`):

| Từ | Điều kiện | Sang |
|---|---|---|
| `home`/`asleep` | `pickGoal` ≠ null | `moving` tới `goal.floor` (lộ trình §7.4) |
| `home` | không mục tiêu, `brief.teamMood === 'sleep'` | `asleep` |
| `asleep` | bất kỳ thay đổi nào của `brief` | `home` |
| `moving` | tới nơi (`now ≥ startAt + route.totalMs`) | `briefing` (goal) hoặc `home` (goal null) |
| `moving`/`briefing` | xuất hiện mục tiêu có rank **nhỏ hơn hẳn** và `now − lastTripAt ≥ TRIP_GAP_MS` | `moving` tới mục tiêu mới (bắt đầu từ vị trí hiện tại) |
| `briefing` | mục tiêu còn (yêu cầu chưa xử lý) | ở lại (không timer) |
| `briefing` | mục tiêu biến mất | đặt `clearedAt`; sau `BRIEF_RETURN_MS`: mục tiêu khác → `moving` tới đó; không có → `moving` về Phòng CEO |
| bất kỳ | `!brief.connected` | `moving` về Phòng CEO (hoặc `home` ngay khi `motion=false`) |

`motion = false`: lộ trình có `totalMs = 0` (nhảy thẳng tới đích), vẫn qua đúng các trạng thái. `wakeAt` là mốc gần nhất trong: hết lộ
trình, `clearedAt + BRIEF_RETURN_MS`, `bubble.until`; không có mốc → `Infinity` (không timer, đúng luật "0 khi rảnh").

### 7.4 Lộ trình (`ui/src/building/route.ts`)

```ts
export type Leg =
  | { k: 'walk'; floor: FloorId; from: Vec3; to: Vec3; ms: number }
  | { k: 'ride'; fromFloor: FloorId; toFloor: FloorId; ms: number };
export interface Route { legs: readonly Leg[]; totalMs: number }
export const WALK_MPS = 1.4;            // Critical: HURRY_MPS = 2.2
export const RIDE_MS_PER_M = 160, RIDE_MIN_MS = 500;
export function planRoute(from: { floor: FloorId; pos: Vec3 }, to: { floor: FloorId; pos: Vec3 }, hurry: boolean, motion: boolean): Route;
export function routePosAt(route: Route, t: number): { floor: FloorId | 'car'; pos: Vec3; walking: boolean; phase: number; carY: number };
```

Cùng tầng: đi theo lối đi (cột khe bàn → `aisleZ` → cột khe bàn đích → `briefSpot`), các đoạn song song trục, không cắt bàn. Khác tầng:
đi tới `ELEVATOR_DOOR[from]` → vào cabin → `ride` (cabin chạy `max(RIDE_MIN_MS, |Δy|·RIDE_MS_PER_M)`) → ra `ELEVATOR_DOOR[to]` → đi tới
đích. Về nhà: đích là `SECRETARY_HOME` tầng 5. **Cabin thang máy thuộc về thư ký** (vị trí cabin = `carY` của lộ trình; đứng yên ở tầng
cuối cùng thư ký tới); chuyển góc nhìn của người dùng không di chuyển cabin.

### 7.5 Lời nói (mẫu cố định, tiếng Việt)

Câu từ `brief.speech` (do `think()` dựng: lỗi, xong việc, yêu cầu mới, mất kết nối, tạm dừng/tiếp tục) hiện nguyên văn trong bong bóng.
Thư ký thêm các câu riêng (khóa chống lặp lưu trong `SecMemory.said`):

| Khi | Câu | Nút |
|---|---|---|
| bắt đầu xuống (rank 0) | `Có việc vùng khóa ở tầng {n} · {tầng}. Em xuống ngay.` | `Xuống tầng {n}` |
| bắt đầu xuống (rank 1–4) | `Em xuống tầng {n} · {tầng} xem {name} nhé.` | `Xuống tầng {n}` |
| tới nơi, yêu cầu Critical | `{name} xin chạy {tool} trong vùng khóa. Anh mở thẻ xem kỹ nhé.` | `Xem thẻ duyệt` |
| tới nơi, yêu cầu khác | `{name} ({vai}) xin chạy {tool} · {mức rủi ro}.` (+ ` Còn {k} yêu cầu nữa.` khi `count > 1`) | `Xem thẻ duyệt` |
| tới nơi, handoff | `Lệnh của {name} dài quá, anh duyệt trong terminal nhé.` | `Xem thẻ duyệt` |
| tới nơi, lỗi | `{name} ở tầng {tầng} gặp lỗi: {act}.` | — |
| về tới Phòng CEO | `Hết việc chờ rồi. Em về Phòng CEO.` (low, bỏ qua khi giờ tập trung) | — |
| người dùng bấm thư ký | `Cả tòa: {w} đang làm, {r} chờ duyệt, {e} lỗi. Bận nhất: tầng {n} · {tầng}.` / `Cả tòa đang rảnh.` | `Xuống tầng {n}` |

`{name}` = `cleanText(name, 32)`, `{tool}` = `cleanText(tool, 24)`, `{act}` = `cleanText(act_text, 80)`, `{vai}` = nhãn danh sách trắng;
`{mức rủi ro}` = `RISK_LABELS` (`ui/src/components/ApprovalCard.tsx`). Không câu nào chứa lệnh. "Xem thẻ duyệt" chỉ hiện khi `canSelect(agent)`.
Bong bóng hiện ở **thanh thư ký DOM** (`role="status"`, `aria-live="polite"`) dưới thanh công cụ của cảnh, kèm sprite "…" trên đầu thư ký trong 3D.

## 8. Tương tác

### 8.1 Góc nhìn

```ts
export type View = 'ext' | FloorId;
export const DEFAULT_VIEW: View = CEO_FLOOR;   // vào cảnh → Phòng CEO
```

**Quyết định**: vào cảnh tòa nhà thì **mặc định xem Phòng CEO** (thấy ngay thư ký, bàn CEO và tường trạng thái của cả 5 tầng), và thanh
công cụ luôn có nút **"Phòng CEO (5)"**. Lý do: người dùng muốn tới thư ký nhanh; tường trạng thái thay vai trò tổng quan của ngoại cảnh;
ngoại cảnh vẫn một nút/phím `E`. Góc nhìn không lưu `localStorage` (mỗi lần mở là Phòng CEO).

Công tắc **"Theo dõi thư ký"** (mặc định tắt, lưu `localStorage` khóa `arcturus.building.follow`, đọc/ghi trong `try/catch`): bật thì khi
thư ký bắt đầu một chuyến rank 0–2, góc nhìn chuyển theo tới tầng đích, về Phòng CEO khi thư ký về. Tắt thì góc nhìn chỉ đổi khi người
dùng chọn (không giật màn hình khi đang đọc thẻ).

### 8.2 Thanh công cụ và bảng thang máy (DOM, trong cảnh)

- Thanh công cụ: `Phòng CEO (5)`, `Ngoại cảnh (E)`, `Đặt lại góc nhìn`, công tắc `Theo dõi thư ký`.
- Bảng thang máy: 6 nút tầng (5 → 0, trên xuống) + `Ngoại cảnh`; mỗi nút: số, tên, plugin, `▶ đang làm`, `! giơ tay`, `+N` tràn, chấm
  vàng "thư ký đang ở đây"; nút Phòng CEO ghi "thư ký · bàn CEO". `aria-pressed` cho tầng đang xem. Màn hình hẹp: bảng thành một hàng cuộn ngang dưới canvas.
- Dưới canvas: `DeskGridScene` chỉ với agent của tầng đang xem (Phòng CEO, ngoại cảnh: mọi agent cần chú ý) để đọc đủ thông tin và dùng
  bàn phím; nút "Xem yêu cầu của X" của nó gọi `onSelect` như cũ.

### 8.3 Chuột

| Thao tác | Ngoại cảnh / tầng khác | Tầng đang xem |
|---|---|---|
| rê | viền sáng tầng + tooltip `Tầng n · Tên · plugin · a/b bàn có người · c đang làm · d giơ tay` | tooltip agent `{name} · {vai đầy đủ} · {MOOD_LABELS}` + `{act}` + `{cwd rút gọn 40}` + "Bấm để mở thẻ duyệt" khi `canSelect`; rê thư ký: "Thư ký · {trạng thái}" |
| bấm | đi tới tầng đó (`goTo(n)`) | bấm agent `canSelect` hoặc sprite "!" → `onSelect(agentId)`; bấm thư ký → câu tóm tắt; bấm màn hình tầng ở tường trạng thái → `goTo(n)` |
| kéo / cuộn | xoay / phóng (OrbitControls: không pan, khoảng cách 9–110, góc cực 0.15–1.48, `rotateSpeed` 0.7, không damping) | như trên |

Tooltip là phần tử DOM, nội dung bằng text node React (không `innerHTML` như mockup). Phân biệt bấm và kéo: di chuyển con trỏ > 5 px = kéo.

### 8.4 Bàn phím

Lắng nghe `onKeyDown` **trên phần tử gốc của cảnh** (có `tabIndex={0}`), không gắn `window`; bỏ qua khi có Ctrl/Alt/Meta hoặc đích là
`input`/`textarea`/`[contenteditable]`. `0`–`5`: chọn tầng; `e`/`E`: ngoại cảnh; `r`/`R`: đặt lại góc nhìn; `f`/`F`: bật/tắt theo dõi thư ký.
Mọi nút DOM đi được bằng Tab; canvas `aria-hidden="true"`; gốc cảnh có `aria-label="Tòa nhà Arcturus, đang xem {tầng}. Phím 0–5 chọn tầng, E ngoại cảnh."`.

### 8.5 Camera (`ui/src/building/camera.ts`, thuần)

```ts
export const FOV = 36, NAV_MS = 1000;
export interface Frame { pos: Vec3; target: Vec3 }
export function frameFor(view: View, aspect: number): Frame;
export function viewTargets(view: View): { lift: readonly number[] /* 7: tầng 0–5 + mái */; cut: readonly number[] /* 6 */; visible: readonly boolean[] };
export function ease(p: number): number;   // easeInOutCubic
```

- `frameFor('ext')`: target (2.5, 13.5, 0), hướng (0.55, 0.5, 0.72), nửa khung (hw 24, hh 19.5) — cao hơn mockup vì thêm tầng 5.
- `frameFor(n)`: target `(cx + 1.4, BASE[n] + (n === 0 ? 1.6 : 0.9), cz + 0.9)`, hướng (0.05, 0.9, 0.5), `hw = w/2 + 3.4`,
  `hh = n === 0 ? 10.5 : d/2 + 1.6`. Khoảng cách `max(hh / tan(FOV/2), hw / (tan(FOV/2)·aspect)) · 1.08`.
- `viewTargets(n)`: `lift[m] = m > n ? LIFT : 0` (mái là chỉ số 6, nâng khi xem bất kỳ tầng nào); `cut[n] = 1` (kính tầng đó trong suốt,
  trần và nan nhôm ẩn, nhãn hiện); `visible[m] = lift[m] < 0.45·LIFT`. Ngoại cảnh: tất cả 0.
- Chuyển góc nhìn là một tween `NAV_MS` nội suy lift, cut, camera.

## 9. Vẽ, vòng đời, ngân sách

### 9.1 Vòng vẽ (`ui/src/building/loop.ts`, không import `three`, test bằng rAF giả)

```ts
export interface Loop {
  invalidate(): void;                                  // xin vẽ một khung (gộp nhiều lần gọi)
  tween(ms: number, fps: 15 | 30, fn: (p: number) => void, key?: string): Promise<void>;
  setMotion(on: boolean): void;                        // false: mọi tween chạy fn(1) ngay
  dispose(): void;
}
export function createLoop(render: () => void, raf?: (cb: FrameRequestCallback) => number, caf?: (id: number) => void): Loop;
```

- Không có `requestAnimationFrame` thường trực. Một khung khi: dữ liệu đổi, resize (`ResizeObserver`), OrbitControls `change`, rê đổi
  vật được chọn. Vòng chỉ chạy khi còn tween; FPS tối đa theo tween nhanh nhất đang chạy: chuyển tầng 30, thư ký đi/cabin chạy 15.
- `motion = false` (trang ẩn hoặc giảm chuyển động, từ prop `motion` = `useMotionPolicy`): tween nhảy tới cuối, vẽ một khung nếu trang
  hiện; trang ẩn thì không vẽ. Trang hiện lại: vẽ một khung. Không tự nghe `visibilitychange` (luật 1 của `docs/UI_EXTENSION.md`).
- Bóng đổ: một `DirectionalLight` 1024², `shadowMap.autoUpdate = false`, chỉ cập nhật khi kết thúc tween hoặc đổi tầng/roster.
- `renderer`: `antialias: true`, `powerPreference: 'low-power'`, `setPixelRatio(min(devicePixelRatio, 1.5))`.

### 9.2 Chi tiết theo góc nhìn

- Chỉ tầng đang xem dựng nội thất đầy đủ, nhân vật chi tiết và nhãn; rời tầng thì gỡ và `dispose()` nhãn/texture của tầng đó; nội thất
  (geometry gộp bằng `mergeGeometries` theo material, mỗi tầng ≤ 30 draw call) dựng lần đầu khi vào tầng rồi giữ lại (giới hạn: giữ tối đa
  2 tầng gần nhất + Phòng CEO, tầng cũ hơn thì `dispose`).
- Ngoại cảnh: vỏ kính, ô cửa sáng theo mood (màn hình InstancedMesh), nhân vật rút gọn, khu đất (cây, ghế, đèn đường InstancedMesh,
  vài khối nhà hàng xóm), trời gradient; sao chỉ ban đêm. Giờ ngày/hoàng hôn/đêm theo giờ máy, tính lại khi vẽ do sự kiện (không timer).
- Ngân sách khung (kiểm tay bằng `renderer.info` trong chế độ dev): ngoại cảnh ≤ 150 draw call, tầng ≤ 220, ≤ 300k tam giác.

### 9.3 Vòng đời và giải phóng

`BuildingCanvas` tạo renderer một lần khi mount; mọi geometry/material/texture đăng ký vào một `Set<{ dispose(): void }>`; unmount:
hủy rAF, `loop.dispose()`, `controls.dispose()`, gỡ listener (`pointer*`, `webglcontextlost`, `ResizeObserver`), `dispose()` cả tập,
`renderer.dispose()`, `renderer.forceContextLoss()`, gỡ canvas. Đổi cảnh sang `live`/`grid` hoặc đóng Office = unmount.

### 9.4 Đóng gói

- `BuildingScene.tsx` và các module thuần nó import **không import `three`**; chỉ `three/*.tsx|ts` import `three`, nạp bằng
  `lazy(() => import('./three/BuildingCanvas'))`. Vite tách chunk; `three` dùng chung chunk với `PetScene`.
- Kiểm: test quét mã nguồn (§12) + `pnpm -C ui build` xem chunk chính không chứa `WebGLRenderer`; ghi kích thước chunk chính trước/sau
  vào Nhật ký (tăng ≤ 15 kB gzip).

### 9.5 Hàng ngân sách đề xuất (thêm vào `docs/BUDGET.md` khi đo, **chưa đo**)

| Thành phần | RAM | CPU | Ghi chú |
|---|---|---|---|
| Văn phòng 3D "Tòa nhà" mở (cảnh `building`, `docs/OFFICE_BUILDING.md`) | ≤ 250 MB private working set cả cây app + WebView2 (gồm tiến trình GPU), không tính hub | ≤ 2 % một nhân khi chuyển tầng/thư ký đi; ≈ 0 % khi đứng yên | 0 FPS khi không tween; ≤ 30 FPS khi chuyển tầng, ≤ 15 FPS khi thư ký đi; đổi cảnh hoặc đóng Office thì RSS về mức Lite (≤ 60 MB) |
| Mở cảnh Tòa nhà | | | ≤ 1,5 s từ lúc bấm "Tòa nhà" tới khung đầu (gồm nạp chunk và dựng Phòng CEO) |

Kịch bản đo đề xuất (cho `budget-guard`, cần ticket lab/app §15): hub + replay 20 phiên có subagent `dev-squad:*`, `tester-squad:*`; mở Office,
chọn Tòa nhà; giữ 60 s ở Phòng CEO; chuyển 0→5→E 3 lần; đóng Office; đo `pws_kb`, CPU, thời gian khung đầu. Lệnh dự kiến:
`pwsh scripts/measure.ps1 -Target office -Scene building`. Chỉ đổi `DEFAULT_SCENE` sang `building` khi hàng này ĐẠT và người duyệt đồng ý.

## 10. Đường lui

```
building ──(không WebGL | import() lỗi | lỗi dựng | webglcontextlost)──► live ──(lỗi render)──► grid
```

- `BuildingScene` kiểm `hasWebGL()` (chuyển từ `ui/src/views/Pet.tsx` sang `ui/src/webgl.ts`) một lần; `false` → render `LiveOfficeScene` với
  cùng props + dòng `muted`: "Máy này không dựng được cảnh 3D, đang hiện văn phòng 2D."
- `<SceneBoundary fallback={...}>` (`ui/src/components/SceneBoundary.tsx`, class component `getDerivedStateFromError`) bọc `Suspense` của canvas:
  lỗi nạp chunk hoặc lỗi render → `LiveOfficeScene`. `BuildingCanvas` gọi `onError(message)` khi tạo renderer lỗi hoặc `webglcontextlost`
  → cùng đường lui. Lỗi được nhớ trong phiên (state) để không thử lại liên tục; lựa chọn "Tòa nhà" trong `localStorage` giữ nguyên, lần mở
  Office sau thử lại.
- Trong `Office`, `SceneBoundary` thứ hai bọc `<Scene>` với fallback `DeskGridScene` (bảo vệ cả `live`).
- Đang nạp chunk: thanh công cụ, bảng thang máy, thanh thư ký và thẻ chi tiết đã dùng được; ô canvas hiện "Đang dựng tòa nhà…".
- Mọi bậc: cột hàng đợi duyệt không đổi, không phụ thuộc cảnh. `docs/UI_EXTENSION.md` hiện ghi "lui về `grid`": sửa thành chuỗi trên.

## 11. Tệp

| Tệp | Mới/đổi | Nội dung |
|---|---|---|
| `ui/src/sceneBrief.ts` (+ `.test.ts`) | mới | `SceneBrief`, `sceneBrief()`, `RISK_RANK` |
| `ui/src/useSceneBrief.ts` (+ `.test.tsx`) | mới | hook giữ `BrainMemory`, gọi `think()` trong effect |
| `ui/src/webgl.ts` | mới | `hasWebGL()` (chuyển từ `views/Pet.tsx`; Pet import lại) |
| `ui/src/components/SceneBoundary.tsx` (+ `.test.tsx`) | mới | error boundary cho cảnh |
| `ui/src/components/DeskGridScene.tsx` | đổi | `OfficeSceneProps.brief?` |
| `ui/src/components/scenes.tsx` | đổi | `SceneId = 'live' \| 'grid' \| 'building'`; `SCENE_LABELS.building = 'Tòa nhà'`; `OFFICE_SCENES.building = BuildingScene`; `isSceneId`; `DEFAULT_SCENE` giữ `live` |
| `ui/src/views/Office.tsx` | đổi | `useSceneBrief(state)` → `brief`; bọc `<Scene>` bằng `SceneBoundary` (fallback `DeskGridScene`). Công tắc thành "Sống / Lưới / Tòa nhà" |
| `ui/src/views/Pet.tsx` | đổi nhỏ | import `hasWebGL` từ `../webgl` |
| `ui/src/index.css` | đổi | token `--floor-lobby`, `--floor-dev`, `--floor-test`, `--floor-rnd`, `--floor-media`, `--floor-ceo` trong `:root` |
| `ui/src/building/floors.ts` (+ test) | mới | §3 |
| `ui/src/building/layout.ts` (+ test) | mới | §4 (GEO, LAY, Phòng CEO, chỗ, lối đi) |
| `ui/src/building/seating.ts` (+ test) | mới | §5 |
| `ui/src/building/figure.ts` (+ test) | mới | §6 (kích thước, `POSES`, `walkAngles`, `partTransforms`, `fnv1a`, `hairStyle`) |
| `ui/src/building/secretary.ts` (+ test) | mới | §7.2–7.3, §7.5 (mẫu câu trong `SECRETARY_LINES`) |
| `ui/src/building/route.ts` (+ test) | mới | §7.4 |
| `ui/src/building/camera.ts` (+ test) | mới | §8.5 |
| `ui/src/building/loop.ts` (+ test) | mới | §9.1 |
| `ui/src/building/useSecretary.ts` | mới | nối `stepSecretary` với React: một `setTimeout` tới `wakeAt`, hủy khi unmount/`motion` đổi |
| `ui/src/building/BuildingScene.tsx` (+ `.test.tsx`) | mới | vỏ DOM: thanh công cụ, bảng thang máy, thanh thư ký, tooltip, `DeskGridScene` của tầng, phím, đường lui; truyền `BuildingModel` xuống canvas |
| `ui/src/building/Building.css` | mới | chỉ dùng token (`styles.test.ts`) |
| `ui/src/building/noThree.test.ts` | mới | quét mã nguồn các module của vỏ, không có `from 'three` |
| `ui/src/building/three/BuildingCanvas.tsx` | mới | `export default`; renderer, OrbitControls, pick, áp `BuildingModel` vào cảnh, `dispose` |
| `ui/src/building/three/world.ts` | mới | khu đất, vỏ tầng, kính, mái + biển, thang máy, cầu nối; registry `dispose` |
| `ui/src/building/three/interiors.ts` | mới | builder nội thất từng tầng (§4.3, §4.4), tường trạng thái |
| `ui/src/building/three/figures.ts` | mới | InstancedMesh nhân vật chi tiết + rút gọn từ `partTransforms` |
| `ui/src/building/three/labels.ts` | mới | `CanvasTexture` nhãn bàn, màn hình tầng, sprite "!" |
| `docs/UI_EXTENSION.md`, `docs/OFFICE.md` | đổi | prop `brief`, cảnh `building`, chuỗi đường lui |
| `.squad/TASKS.md` | đổi | dòng Nhật ký |

Hợp đồng vỏ → canvas (`ui/src/building/model.ts`, kiểu thuần):

```ts
export interface BuildingModel {
  view: View;
  rosters: readonly FloorRoster[];                     // tầng 0…4
  secretary: { mem: SecMemory; startAt: number };      // canvas gọi routePosAt/secretaryPose để vẽ
  selectable: ReadonlySet<AgentId>;                    // = canSelect, dùng cho sprite "!"
  pending: ReadonlyMap<AgentId, PendingBrief>;         // tường trạng thái
  timeMode: 'day' | 'dusk' | 'night';
}
export type PickHit =
  | { k: 'floor'; floor: FloorId } | { k: 'agent'; agentId: AgentId } | { k: 'bang'; agentId: AgentId }
  | { k: 'secretary' } | { k: 'status-screen'; floor: AgentFloorId };
export interface BuildingCanvasProps {
  model: BuildingModel;
  motion: boolean;
  onHover: (hit: PickHit | null, x: number, y: number) => void;
  onPick: (hit: PickHit) => void;       // vỏ quyết: goTo / onSelect (chỉ khi agent ∈ selectable) / tóm tắt
  onReady: () => void;
  onError: (message: string) => void;
}
```

Canvas không biết `onSelect`, không biết `brief`; chỉ báo vật được bấm. Vỏ là nơi duy nhất gọi `onSelect`.

## 12. Test bắt buộc

- `floors.test.ts`: bảng §3.3 đầy đủ; không đầu vào nào (kể cả fuzz 1.000 chuỗi ngẫu nhiên có `:`, `/`, ký tự ẩn, chữ có dấu, rất dài)
  trả tầng ngoài 0–4 hay `roleId` ngoài danh sách trắng; `FLOORS.length === 6`, tầng 5 không có `plugin` và `roles` rỗng; id vai trò không trùng giữa các tầng.
- `layout.test.ts`: `BASE`/`ROOF_Y` đúng; mọi bàn, chỗ nghỉ, chỗ đứng nằm trong sàn tầng của nó; bàn không chồng nhau; Phòng CEO nằm
  trong mặt bằng tầng 4 (tầng trên không đua ra ngoài tầng dưới); `SECRETARY_HOME`, bàn CEO, 5 màn hình tường nằm trong tầng 5.
- `seating.test.ts`: bàn ổn định khi mood đổi; trả bàn khi agent bị xóa; hai `developer` → một bàn vai trò, một bàn linh hoạt; khách vào
  sofa Sảnh; quá chỗ → `overflow` và tối đa 24 hình/tầng; idle → góc nghỉ, idle lâu → `sleep`; bảng mood/activity → tư thế; `stats` đúng.
- `figure.test.ts`: `raise` tay phải cao hơn đỉnh đầu; `sit_*` hông ≈ 0.45 và bàn tay `sit_type` ở trên mặt bàn (y ≈ 0.75–0.85) phía trước
  thân; `stand` chân chạm đất (đáy giày y ≈ 0); `walkAngles` tuần hoàn (pha 0 = pha 1) và hai chân ngược pha; `hairStyle` ổn định theo id;
  bảng màu sai dạng → `DEFAULT_PALETTE`.
- `secretary.test.ts`:
  - không có việc → ở `home` tầng 5, `wakeAt = Infinity`; đội rảnh lâu → `asleep`, có thay đổi → thức;
  - Critical ở tầng 2 → `moving` (có chặng `ride` 5→2) → `briefing`, câu Critical, nút "Xem thẻ duyệt" chỉ khi `canSelect`;
  - đang tới yêu cầu Low thì có Critical → đổi đích; có thêm Low khi đang ở Critical → không đổi;
  - yêu cầu xong → sau `BRIEF_RETURN_MS` sang mục tiêu kế hoặc về Phòng CEO;
  - giờ tập trung: Medium/Low, handoff, lỗi không làm thư ký đi; Critical/High vẫn đi;
  - mất kết nối → về Phòng CEO, không đi đâu;
  - `motion=false` → tới đích ngay, cùng dãy trạng thái;
  - **không câu nào chứa `command_normalized`/`command_original`** của yêu cầu (dựng fixture có lệnh đặc biệt và kiểm không xuất hiện);
  - tên chứa ký tự ẩn hiện thành `⟨U+XXXX⟩`;
  - **module không có đường gửi**: `stepSecretary` không nhận/không trả `ClientMsg`; `BubbleAction` chỉ có `select` | `view` (test kiểu bằng `expectTypeOf`).
- `route.test.ts`: cùng tầng không có chặng `ride`; khác tầng có đúng một `ride`; các đoạn đi bộ không cắt hình chữ nhật bàn; `routePosAt`
  liên tục, `t ≥ totalMs` → đích; `motion=false` → `totalMs = 0`.
- `camera.test.ts`: `viewTargets(5)` chỉ nâng mái; `viewTargets(2)` nâng 3, 4, 5, mái; `ext` không nâng gì; `frameFor` chứa đủ khung tầng
  theo aspect (điểm góc sàn chiếu vào trong [-1, 1]); `ease(0)=0`, `ease(1)=1`.
- `loop.test.ts` (rAF giả): `invalidate` nhiều lần = một khung; không tween thì không có rAF chờ; tween 30/15 FPS bỏ khung đúng; `setMotion(false)`
  → `fn(1)` ngay, không rAF; `dispose` hủy rAF đang chờ.
- `sceneBrief.test.ts` / `useSceneBrief.test.tsx`: rủi ro cao nhất và `tool` của yêu cầu cũ nhất; handoff; không có trường lệnh trong
  kết quả; StrictMode vẫn ra câu nói (không mất vì gọi hai lần).
- `BuildingScene.test.tsx` (canvas giả qua prop `Canvas` như `PetView` nhận `Scene`):
  - mặc định xem Phòng CEO; phím `0`–`5`, `E` đổi góc nhìn; nút "Phòng CEO (5)" luôn có;
  - `webgl=false` → hiện `LiveOfficeScene` + dòng báo; canvas gọi `onError` → lui về `live`; chunk ném lỗi → lui về `live`;
  - `onPick({k:'bang'})` → `onSelect(id)` đúng một lần; agent không `canSelect` → không gọi; **không bao giờ gọi `onSelect` khi không có thao tác người dùng** (đổi `brief` liên tục, chạy timer giả);
  - thanh thư ký hiện câu; nút "Xem thẻ duyệt" gọi `onSelect`; tên agent độc (`<img onerror>`) hiện như chữ;
  - bảng thang máy hiện số đúng, `+N`, chấm thư ký; agent có `role: 'ceo:x'` không xuất hiện ở tầng 5.
- `scenes.test.tsx`: `isSceneId('building')`, nhãn "Tòa nhà"; `Office.test.tsx`: công tắc 3 cảnh; cảnh ném lỗi → `DeskGridScene`, cột duyệt vẫn bấm được.
- `noThree.test.ts`: `BuildingScene.tsx`, `floors/layout/seating/figure/secretary/route/camera/loop/model.ts`, `sceneBrief.ts`, `useSceneBrief.ts`,
  `scenes.tsx` không chứa `from 'three` hay `import('three`.
- `styles.test.ts` xanh với `Building.css`.

## 13. Kế hoạch triển khai (một người, một lượt; mỗi bước `pnpm -C ui test` + `pnpm -C ui typecheck` xanh)

1. `webgl.ts`, `SceneBoundary`, `sceneBrief.ts`, `useSceneBrief.ts`, prop `brief?`; `Office` truyền `brief` và bọc `SceneBoundary`. Kiểm: test cũ của Office/Pet xanh.
2. Module thuần `floors`, `layout`, `seating`, `figure`, `camera`, `route`, `secretary`, `loop`, `model` + test §12. Chưa có UI.
3. `BuildingScene` vỏ DOM với canvas giả + `useSecretary`; thêm khóa `building` vào `scenes.tsx` (mặc định giữ `live`); `Building.css`, token.
   Kiểm: `BuildingScene.test.tsx`, `noThree.test.ts`, `pnpm -C ui build` (chunk chính không có `WebGLRenderer`).
4. `three/world.ts` + `BuildingCanvas`: vỏ tòa nhà 6 tầng, mái + biển, thang máy, camera/OrbitControls, vòng vẽ theo nhu cầu, pick, `dispose`.
   Kiểm tay trên Vite dev (`?view=office`, agent giả): chuyển tầng, ngoại cảnh, tab ẩn = 0 khung (DevTools Performance).
5. `three/interiors.ts` (5 tầng + Phòng CEO, tường trạng thái), `three/labels.ts`.
6. `three/figures.ts`: nhân vật chi tiết + rút gọn, tư thế, thư ký và cabin theo lộ trình.
7. Tài liệu: `docs/UI_EXTENSION.md`, `docs/OFFICE.md`, Nhật ký `.squad/TASKS.md`; ảnh chụp trước/sau (Office rộng, 390 px), kiểm bàn phím và
   `prefers-reduced-motion`. Đo ngân sách (§9.5) là ticket riêng; không đổi `DEFAULT_SCENE`.

## 14. Câu hỏi mở

1. **Hook-lab (làn `lab`)**: `agent_type` của subagent do plugin cung cấp có đúng dạng `<plugin>:<agent>` không (ví dụ `dev-squad:developer`)?
   Có dạng lồng (`plugin:dir:agent`) không? Fixture hiện chỉ có `general-purpose` và `""` (`fixtures/hooks/`). Kết quả quyết bước 3–5 của §3.2.
2. `MATCH_BARE_ROLE = true`: agent riêng của người dùng trùng tên vai trò (ví dụ `developer`) sẽ ngồi ở tầng Dev. Chấp nhận (chỉ hiển thị) hay tắt?
3. Plugin lạ vào khu khách ở Sảnh (đã chọn) hay muốn một tầng "Khách" cố định thứ bảy?
4. Phiên chính đang chạy một lệnh plugin (ví dụ `/dev-squad:feature`) vẫn ở Sảnh. Có muốn nó lên tầng đội của nó (cần hub, §15) không?
5. "Theo dõi thư ký" mặc định tắt (đã chọn) — đồng ý?
6. Tên hiển thị của thư ký: "Thư ký" (đã dùng) hay tên riêng do người dùng đặt (Pet hiện chưa có tên)?
7. Có cần nút chọn giờ ngày/đêm như mockup, hay chỉ theo giờ máy (đã chọn)?
8. Có lưu mockup HTML vào repo (`docs/mockups/`) làm tài liệu hình ảnh không?

## 15. Đề xuất ticket (không tự tạo)

| Tóm tắt | Làn | Story cha |
|---|---|---|
| Cảnh `building`: tòa nhà 3D 6 tầng, Phòng CEO, thư ký trưởng (tài liệu này) | ui | T13 · ARC-34 |
| Hook-lab: kiểm `agent_type` của subagent plugin và agent teams (Q9), thêm fixture | lab | T1 · ARC-22 |
| Đo ngân sách cảnh Tòa nhà: `measure.ps1 -Target office -Scene building` + chế độ đo Office trong app, ghi hàng §9.5 | lab + ui (`crates/app`) | T16 · ARC-37 |
| Hub: `AgentView.team` (plugin tách từ `agent_type`, kiểm ký tự, ≤ 64) và cắt `role` ≤ 128 ký tự (hiện không cắt) — cập nhật `docs/PROTOCOL.md` | hub | T4 · ARC-25 |
| Hub (cân nhắc riêng tư): suy đội của phiên chính từ lệnh slash plugin trong `UserPromptSubmit` | hub | T4 · ARC-25 |
| Nghiên cứu: đồng bộ vị trí/lời thư ký giữa cửa sổ Office và Pet (cùng nguồn, tránh nói đôi) | ui | Epic ARC-6 (v1) |
| Cấu hình người dùng: ánh xạ plugin → tầng trong `arcturus.toml` | hub + ui | T4 · ARC-25 |

## 16. Rủi ro và cách giảm

| Rủi ro | Giảm |
|---|---|
| RAM/GPU của WebView2 vượt ngân sách trên máy yếu | Opt-in, không mặc định; chi tiết chỉ tầng đang xem, giữ ≤ 3 tầng nội thất; pixel ratio ≤ 1.5; đo trước khi bật mặc định |
| Định dạng `agent_type` khác giả định → mọi subagent plugin vào Sảnh | Luật khớp tên trần (§3.2 bước 5); câu hỏi Q1 cho hook-lab; đề xuất trường `team` ở hub |
| Vòng vẽ quên dừng (OrbitControls, tween treo) | `loop.ts` có test; không damping; tween có hạn; chế độ dev log số khung khi đứng yên |
| Rò WebGL context khi đổi cảnh nhiều lần | Registry `dispose`, `forceContextLoss`; kiểm tay đổi cảnh 20 lần, `renderer.info.memory` về 0 |
| Thư ký gây nhiễu (đi liên tục, nói nhiều) | `TRIP_GAP_MS`, chỉ đổi đích khi rank thấp hơn hẳn, giờ tập trung, chống lặp khóa, bong bóng tự ẩn |
| Hiểu nhầm "thư ký điều phối" thành tự duyệt/giao việc | Bất biến ADR 0003; kiểu `BubbleAction` không có hành động gửi; test không tự gọi `onSelect` |
| Chữ tiếng Việt trên canvas lỗi font | Chờ `document.fonts.ready`; fallback `sans-serif`; thông tin đầy đủ vẫn ở DOM (tooltip, thẻ chi tiết) |
| Cảnh 3D lỗi làm trống Office | Đường lui `building → live → grid`; hàng đợi duyệt độc lập |
