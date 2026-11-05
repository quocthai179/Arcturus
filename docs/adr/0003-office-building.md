# ADR 0003 — Văn phòng 3D "Tòa nhà Arcturus" có thư ký trưởng điều phối

- Trạng thái: **đề xuất, người dùng đã duyệt mockup 2026-11-05**. Chưa có mã ARC (đề xuất ticket dưới story T13 · ARC-34,
  làn `ui`; xem `docs/OFFICE_BUILDING.md` §13).
- Phạm vi: chỉ `ui/` (làn `ui`). Không đổi hub, schema, giao thức, `crates/app/`, CSP.
- Thiết kế chi tiết: `docs/OFFICE_BUILDING.md`. Liên quan: `docs/UI_EXTENSION.md`, `docs/OFFICE_LIVE.md`, `docs/BUDGET.md`,
  `docs/adr/0001-pet-v1.md`, `docs/adr/0002-pet-orchestrator-tier2.md`, `CLAUDE.md` quy tắc bất biến 2, 3, 5, 6.

## Bối cảnh

Văn phòng hiện có hai cảnh qua điểm cắm `OFFICE_SCENES` (`ui/src/components/scenes.tsx`): `live` (sàn 2D SVG, mặc định,
`docs/OFFICE_LIVE.md`) và `grid`. Khi số agent tăng và người dùng chạy nhiều bộ plugin squad (`dev-squad`, `tester-squad`,
`lab`, `demo-builder`), một sàn phẳng 12 chỗ không còn cho thấy "ai thuộc đội nào".

Người dùng đã duyệt một mockup 3D (một file HTML dùng three r128, dữ liệu mẫu): **tòa nhà = máy này, mỗi tầng = một bộ plugin
squad**, tòa kính kiểu startup, thang máy kính bên ngoài, bấm tầng thì các tầng trên nâng lên để nhìn mặt cắt, hàng đợi duyệt
luôn ở cột riêng ngoài cảnh. Người dùng yêu cầu làm thật trong app, thêm hai điểm: (1) nhân vật và nội thất chi tiết hơn;
(2) văn phòng được **thư ký trưởng** "điều phối": chính là trợ lý Pet, người nói chuyện với người dùng.
Thay đổi của người dùng cùng ngày: thư ký **không** ngồi ở Sảnh mà làm việc ở **tầng trên cùng, Phòng CEO** của người dùng
(người dùng là CEO); Sảnh chỉ có quầy lễ tân bình thường.

Ràng buộc có sẵn:

- Hợp đồng cảnh: cảnh không đọc store/WebSocket, không gửi lệnh, chọn agent giơ tay thì gọi `onSelect`; hàng đợi duyệt
  không thuộc cảnh, cảnh lỗi không được chặn duyệt (`scenes.tsx`, `docs/UI_EXTENSION.md`).
- Pet không bao giờ tự duyệt, không gửi `decide*`, `batch_*`, `suggestion_answer` (`PetClientMsg` ở `ui/src/pet/commands.ts`,
  PLAN §7.2). Tầng 2 (LLM giao việc) chưa được duyệt (ADR 0002).
- 0 token (quy tắc 5); chuỗi từ agent là dữ liệu không tin cậy (quy tắc 3).
- Ngân sách: bộ vẽ nặng nạp muộn, không `requestAnimationFrame` thường trực, đo trước khi bật mặc định (`docs/UI_EXTENSION.md`
  luật 2–4, `docs/BUDGET.md`).
- Hub chỉ cho UI `AgentView.role` = `agent_type` thô của subagent (`crates/hub-core/src/state.rs` `new_subagent_runtime`, không cắt
  độ dài), `kind`, `parent_id`; không có trường "đội/plugin".

## Quyết định

1. **Cảnh thứ ba `building` trong `OFFICE_SCENES`**, nhãn công tắc "Tòa nhà". Mặc định vẫn là `live` cho tới khi có số đo ngân
   sách (§9 của thiết kế); sau đó người duyệt quyết có đổi mặc định không.
2. **Tách hai lớp**: vỏ DOM nhẹ `ui/src/building/BuildingScene.tsx` (bundle chính: bảng thang máy, thanh thư ký, tooltip, thẻ chi
   tiết tầng đang xem) và phần three.js `ui/src/building/three/BuildingCanvas.tsx` nạp bằng `React.lazy(() => import(...))`, chunk
   riêng (dùng chung chunk `three` với `PetScene`). Mọi logic có thể test (xếp tầng, xếp chỗ, tư thế, camera, lộ trình, thư ký)
   là module TS thuần không import `three`.
   Vào cảnh thì **mặc định xem Phòng CEO** (nơi có thư ký và tường trạng thái); thanh công cụ luôn có nút "Phòng CEO" (phím `5`)
   và "Ngoại cảnh" (phím `E`); phím `0`–`5` chọn tầng.
3. **Xếp tầng bằng danh sách trắng cố định, không bao giờ sinh tầng mới từ chuỗi agent.** Có đúng 6 tầng: 0 Sảnh, 1 Dev, 2 Test,
   3 R&D, 4 Media và **5 Phòng CEO**. Tầng 5 cố định, không gắn plugin nào, **không có đường nào từ `role`/`kind` tới tầng 5**
   (hàm xếp tầng chỉ trả kiểu `AgentFloorId = 0 | 1 | 2 | 3 | 4`). `role` được kiểm ký tự/độ dài rồi tra bảng `PLUGIN_FLOORS` (so chính xác, dùng `Map`). Plugin lạ, chuỗi sai
   dạng, quá dài → khu **khách** ở Sảnh. Vai trò lạ trong plugin đã biết → bàn linh hoạt (hot desk) của tầng đó. Chuỗi chỉ để hiện
   chữ, không dùng cho quyết định nào ngoài chọn chỗ ngồi.
4. **Thư ký trưởng = danh tính của Pet, chạy ở tầng 0/1, chỉ trong cảnh. Chỗ làm của nó là bàn chỉ huy ở Phòng CEO (tầng 5)**,
   cạnh bàn CEO luôn để trống (người dùng là CEO), trước bức tường trạng thái các tầng (mỗi tầng một màn hình: số đang làm,
   số giơ tay) và cửa kính nhìn ra thành phố; biển ARCTURUS trên mái ngay trên phòng. Có việc thì thư ký đi thang máy xuống tầng
   cần chú ý; hết việc chờ thì quay về Phòng CEO. Sảnh giữ phiên chính và agent `general-purpose`, có quầy lễ tân bình thường,
   không có Pet. Nó quan sát sự kiện (qua dữ liệu `brief` do `Office`
   truyền vào), nói bằng văn bản mẫu (dùng lại `think()` của `ui/src/pet/brain.ts` + mẫu câu theo tầng), tự đi bộ/đi thang máy tới
   tầng cần chú ý, tóm tắt, và đưa người dùng tới thẻ duyệt **chỉ khi người dùng bấm** (gọi `onSelect`). Nó **không gửi bất kỳ
   thông điệp nào lên hub** (cảnh không có `send`); lệnh đội tầng 1 vẫn ở `TeamControls` và cửa sổ Pet. Giao việc cho agent bằng LLM
   là tầng 2 (ADR 0002, chưa duyệt), **ngoài phạm vi**.
5. **Mở rộng `OfficeSceneProps` bằng một prop tùy chọn chỉ đọc `brief?: SceneBrief`** (kết nối, tạm dừng, giờ tập trung, yêu cầu chờ
   theo agent gồm `tool` + `risk` + số lượng, handoff, câu nói tầng 0). `Office` dựng nó bằng hàm thuần `ui/src/sceneBrief.ts` và
   hook `ui/src/useSceneBrief.ts`. Cảnh vẫn không đọc store; `brief` không chứa `command_*` (quy tắc 3: thư ký không đọc lệnh).
6. **Nhân vật và nội thất là hình học thủ tục**, không tài sản ngoài: nhân vật 23 phần (đầu, tóc 3 kiểu, mắt, miệng, cổ, thân,
   thắt lưng, phù hiệu, tay 2 đốt, bàn tay, chân 2 đốt, giày) với bảng góc khớp cho 10 tư thế; màu từ `AgentView.palette` đã kiểm
   `#RRGGBB`. Vẽ bằng `InstancedMesh` (một mesh cho mỗi phần), chi tiết **chỉ ở tầng đang xem**; ngoại cảnh dùng hình rút gọn.
7. **Vẽ theo nhu cầu**: không vòng lặp thường trực; một khung khi dữ liệu/kích thước/chuột đổi; vòng ngắn chỉ trong tween (chuyển
   tầng ≤30 FPS, thư ký đi ≤15 FPS); trang ẩn hoặc giảm chuyển động thì nhảy thẳng tới trạng thái cuối, vẽ một khung. Gỡ cảnh thì
   `dispose()` mọi geometry/material/texture, `forceContextLoss()`.
8. **Đường lui ba bậc**: không có WebGL, chunk lỗi, lỗi dựng cảnh hay mất context → `LiveOfficeScene` với cùng props (không dùng
   WebGL); nếu chính cảnh đó lỗi → `DeskGridScene`. Hàng đợi duyệt không bị ảnh hưởng ở mọi bậc.

## Các phương án đã cân nhắc

| Phương án | Ưu | Nhược |
|---|---|---|
| A. three.js thủ tục, lazy chunk, vẽ theo nhu cầu (chọn) | Đúng mockup đã duyệt; `three` 0.180.0 đã là phụ thuộc (Pet); không tài sản ngoài, không giấy phép; kiểm soát được vòng vẽ | Tốn RAM GPU khi mở; phải tự quản lý `dispose`; cần đo ngân sách |
| B. Isometric SVG/CSS 2.5D mở rộng từ `live` | Không WebGL, rẻ; dùng lại `live/` | Không có thang máy kính, mặt cắt, xoay camera mà người dùng đã duyệt; nhiều tầng SVG sẽ nặng DOM |
| C. Thư viện React cho three (`@react-three/fiber`, `drei`) | Viết khai báo nhanh | Thêm phụ thuộc lớn; vòng `useFrame` mặc định chạy liên tục (phải cấu hình `frameloop="demand"`); thêm lớp khó soát `dispose` |
| D. Model glTF/VRM có sẵn cho nhân vật và nội thất | Đẹp ngay | Giấy phép từng tài sản, dung lượng, CSP `blob:`; ADR 0001 điểm 9 đã từ chối VRM ngoài cửa sổ pet vì ngân sách |
| Xếp tầng: hub thêm trường `team` | Một nguồn sự thật, UI đơn giản | Đổi schema/giao thức, ngoài làn `ui`. Ghi thành đề xuất ticket (thiết kế §13), UI làm trước bằng danh sách trắng |
| Thư ký ở quầy lễ tân Sảnh (như mockup) | Gần phiên chính | Người dùng đã đổi: thư ký thuộc Phòng CEO; Sảnh đông agent, khó nhìn thư ký |
| Vào cảnh mặc định ở Ngoại cảnh | Thấy cả tòa ngay | Người dùng muốn tới thư ký nhanh; Phòng CEO có tường trạng thái nên vẫn thấy cả tòa. Ngoại cảnh có nút/phím `E` |
| Plugin lạ: tạo tầng thêm động | Ai cũng có "tầng" riêng | Chuỗi không tin cậy sinh hình học không giới hạn; trái yêu cầu. Thay bằng khu khách ở Sảnh |
| Thư ký gửi lệnh đội từ cảnh (`team_pause`…) | "Điều phối" trọn vẹn hơn | Phá hợp đồng cảnh (cảnh không gửi lệnh); đã có `TeamControls` ngay trên cảnh và lệnh tầng 1 ở cửa sổ Pet |

## Hệ quả

- Không thêm phụ thuộc mới. `three/examples/jsm/controls/OrbitControls.js` và `BufferGeometryUtils` nằm trong gói `three` sẵn có.
- `OfficeSceneProps` có thêm `brief?` (tùy chọn, tương thích ngược: `live`, `grid` bỏ qua). `docs/UI_EXTENSION.md` phải ghi
  prop mới và đường lui `building → live → grid` (hiện ghi "lui về `grid`").
- `hasWebGL()` chuyển từ `ui/src/views/Pet.tsx` sang `ui/src/webgl.ts` để Pet và cảnh tòa nhà dùng chung.
- Cửa sổ Office có thể giữ một WebGL context khi đang ở cảnh tòa nhà: cần hàng ngân sách riêng và lệnh đo mới (đề xuất ticket);
  chưa được bật mặc định khi chưa đo.
- Thư ký trong Office và Pet ở cửa sổ riêng là hai bản chạy độc lập cùng mẫu câu; có thể cùng nói một tin. Thư ký trong Office
  chỉ hiện chữ, không đọc giọng (tránh nói đôi với Pet).
- Nếu hook-lab cho thấy `agent_type` của subagent plugin không có dạng `<plugin>:<agent>`, bảng xếp tầng vẫn chạy nhờ luật khớp tên
  vai trò trần (thiết kế §3.2), nhưng cần kiểm lại.

## Bất biến (vi phạm = chưa xong)

1. **Không bao giờ tự duyệt.** Cảnh và thư ký không có đường tới `send`; không gọi `decide`, `decide_group`, `batch_open`,
   `batch_close`, `suggestion_answer`, kể cả gián tiếp. `onSelect` chỉ chạy sau cú bấm/Enter của người dùng, không bao giờ tự gọi.
2. **Hàng đợi duyệt ở ngoài cảnh** (`QueueList` trong cột `office__queue`). Mọi yêu cầu thư ký nhắc đều có thẻ ở đó; không có thông
   báo duyệt nào chỉ tồn tại trong cảnh. Cảnh tải lỗi, không WebGL hay treo thì vẫn duyệt được.
3. **Tầng 5 không nhận agent**: không chuỗi nào từ agent đưa được nhân vật vào Phòng CEO; ở đó chỉ có thư ký và bàn CEO trống.
4. **Thư ký không hiện lệnh**: chỉ `tool` và mức rủi ro; muốn xem lệnh thì mở thẻ (quy tắc 3). Không có lời gợi ý "đồng ý".
5. **0 token**: mọi câu là văn bản mẫu; không gọi LLM, không gọi mạng ngoài hub.
6. **Chuỗi không tin cậy chỉ để hiện chữ**: `role`, `name`, `act_text`, `cwd` đi qua `cleanText()` (ký tự ẩn hiện ra, cắt độ dài) và
   hiện bằng text node React hoặc `fillText` lên canvas; không `innerHTML`, không làm khóa object thường, không sinh tầng/khu mới.
7. **Ngân sách**: không vòng vẽ khi đứng yên, chi tiết chỉ ở tầng đang xem, `dispose` đủ khi gỡ; không bật mặc định trước khi đo.
8. **Không đổi hub, giao thức, `crates/app/`**; đổi `OfficeSceneProps` thì cập nhật `docs/UI_EXTENSION.md` cùng commit.
