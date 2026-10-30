# Văn phòng sống (cảnh `live`) — kế hoạch và thiết kế

Yêu cầu người dùng (2026-10-29): văn phòng 2D tĩnh một chỗ là chưa đủ. Muốn một văn phòng "như thật", có hoạt ảnh:
agent **rảnh thì đi dạo, tán gẫu với nhau**, **có việc thì quay về bàn làm việc**.

Chưa có mã ARC (đề xuất ticket mới dưới story T13 · ARC-34, làn `ui`).

## 1. Phạm vi

- Chỉ chạm lớp trình bày `ui/` (làn `ui`), qua điểm cắm có sẵn `OFFICE_SCENES` (docs/UI_EXTENSION.md). Không đổi hub,
  giao thức, `store.ts`, `useApprovalQueue`, `allowGate`, `countdown.ts`.
- Cảnh mới `live` thành mặc định; cảnh `grid` cũ vẫn còn, chọn được bằng công tắc "Sống / Lưới" ở đầu Office
  (lưu ở `localStorage`, đọc/ghi trong `try/catch`).
- Hàng đợi duyệt vẫn ở cột riêng (`QueueList`), không phụ thuộc cảnh. Thẻ chi tiết từng agent (`AgentDesk`) vẫn hiện
  dưới sàn văn phòng để đọc đủ vai trò, hoạt động, context, nguồn/máy (và dùng bàn phím).

## 2. Bố cục sàn (toạ độ logic 1000 × 620, lưới ô 20 px)

```
┌──────────── tường: cửa sổ (trời theo giờ máy), đồng hồ, bảng tên ARCTURUS ────────────┐
│  [bàn1] [bàn2] [bàn3] [bàn4]        ║   PHÒNG NGHỈ: sofa 3 chỗ, thảm, bàn trà, cây      │
│                                      ║   ghế lười (bean bag)                             │
│  [bàn5] [bàn6] [bàn7] [bàn8]        ║────────────────────────────────────────────────── │
│                                      ║   PANTRY: máy cà phê, bình nước, tủ lạnh,        │
│  bảng trắng   [bàn họp 4 chỗ = hot desk]   kệ sách     bàn tròn 2 ghế                    │
└───────────────────────────────────────────────────────────────────────────────────────┘
```

- 8 bàn làm việc cố định + 4 chỗ "hot desk" quanh bàn họp. Mỗi agent giữ **một bàn ổn định** theo thứ tự xuất hiện
  (không đổi chỗ khi mood đổi); agent bị xoá thì bàn được trả lại. Agent thứ 13 trở đi không có chỗ ngồi: vẫn đi lại,
  khi làm việc thì đứng cạnh bảng trắng. Tối đa 16 nhân vật trên sàn, còn lại hiện chip "+N agent khác" (ngân sách).
- Các điểm đến (POI): máy cà phê, bình nước, cửa sổ, cây, kệ sách, bảng trắng, sofa, ghế lười, bàn tròn pantry. Mỗi
  điểm hội thoại có 2 vị trí đứng đối diện nhau.
- Đồ đạc là vật cản trên lưới ô; đường đi tìm bằng A* (8 hướng, không cắt góc) rồi kéo thẳng (line of sight) cho ít
  điểm gấp khúc.

## 3. Máy trạng thái của một nhân vật (`ui/src/live/sim.ts`, hàm thuần, có test)

| Mood hub | Đích | Hành vi khi tới |
|---|---|---|
| `working` | bàn của mình (chạy nhanh ×1,9) | ngồi gõ phím, laptop sáng tím |
| `approval` | bàn của mình (chạy nhanh) | đứng giơ tay, bong bóng "!" bấm được → `onSelect` |
| `error` | bàn của mình (chạy nhanh) | ngồi, laptop đỏ, giọt mồ hôi |
| `done` | ở bàn ăn mừng ~4 s | rồi như `idle` |
| `idle` | giờ giải lao | đi dạo / tán gẫu, hết giờ giải lao thì ngủ gật ở sofa/ghế lười/bàn |

Hành vi khi rảnh (chọn ngẫu nhiên có trọng số, rng gieo hạt để test được):

- **Đi dạo**: tới một POI còn trống, đứng 4–9 s, có thể nói một câu một mình ("Cây này tưới chưa nhỉ…").
- **Tán gẫu**: ghép đôi với một agent rảnh khác chưa bận, cùng đi tới một điểm hội thoại; ai tới trước thì chờ (tối
  đa 12 s). Đủ hai người thì luân phiên bong bóng theo một kịch bản 2–4 câu "xàm xí" (mỗi câu ~2,8 s), xong tách ra.
- **Bị gọi đi làm**: mood chuyển sang làm việc giữa lúc đi dạo/tán gẫu → bong bóng "Có việc rồi!", chạy về bàn; người
  kia nói "Ờ, đi đi" rồi tự đi dạo tiếp.
- Câu thoại là chuỗi tĩnh do chúng ta viết (`ui/src/live/chatter.ts`), không lấy nội dung từ agent. Tên agent (dữ liệu
  không tin cậy) chỉ hiện qua React text node.

## 4. Hoạt ảnh và ngân sách (docs/BUDGET.md "Văn phòng 2D", docs/UI_EXTENSION.md "Luật chuyển động")

- **Không có vòng `requestAnimationFrame`.** Mỗi đoạn đường đi là một CSS `transition: transform <thời lượng> linear`
  trên `<g>` SVG; trình duyệt tự nội suy. JS chỉ chạy ở **mốc sự kiện**: hết một đoạn đường, đổi câu thoại, hết thời
  gian đứng. Một `setTimeout` duy nhất trỏ tới mốc gần nhất; không còn mốc nào thì không có timer.
- Bước chân/nhún người là CSS keyframes chỉ gắn khi đang đi; gõ phím chỉ khi đang làm việc.
- **Giờ giải lao có hạn**: mỗi agent rảnh đi dạo tối đa `WANDER_MS` = 5 phút kể từ lúc rảnh (hoặc từ lúc người dùng
  "gọi cả đội dậy"). Hết giờ thì về chỗ ngủ gật (tư thế tĩnh). Khi mọi agent đều rảnh và đã ngủ: **0 timer, 0
  animation** — đúng dòng "0 khi tất cả rảnh" của BUDGET.md sau một khoảng giải lao. Nút "Giải lao tiếp" đánh thức
  để xem lại.
- Trang ẩn hoặc `prefers-reduced-motion`: `useMotionPolicy` trả `false` → mô phỏng dừng hẳn (không timer), nhân vật
  được đặt thẳng vào vị trí đích (bàn hoặc chỗ ngủ), CSS toàn cục đã tắt transition/animation.
- Không thư viện mới, không canvas/WebGL; một SVG duy nhất co theo `viewBox`.
- Kích thước DOM: ≤16 nhân vật × ~40 phần tử.

## 5. Tệp

| Tệp | Nội dung |
|---|---|
| `ui/src/live/geometry.ts` | lưới ô, vật cản, A*, kéo thẳng đường |
| `ui/src/live/layout.ts` | bố cục: bàn, hot desk, POI, chỗ ngủ, vật cản |
| `ui/src/live/chatter.ts` | kịch bản tán gẫu, câu một mình, câu khi bị gọi đi làm |
| `ui/src/live/sim.ts` | mô phỏng thuần: `createSim`, `syncAgents`, `advance`, `nextWakeAt`, `wakeAll` |
| `ui/src/live/useLiveSim.ts` | nối mô phỏng với React: một timer tới mốc gần nhất, dừng khi không được chuyển động |
| `ui/src/live/LiveOfficeScene.tsx` | vẽ sàn, đồ đạc, nhân vật, bong bóng; xếp lớp theo trục y |
| `ui/src/live/Walker.tsx` | nhân vật có chân (dùng lại `Character`), tư thế đi/ngồi/đứng/ngủ |
| `ui/src/live/LiveOffice.css` | keyframes bước chân, gõ phím, bong bóng (chỉ dùng token) |
| `ui/src/components/scenes.tsx` | thêm khóa `live`, mặc định `live`; prop tuỳ chọn `motion` |
| `ui/src/views/Office.tsx` | công tắc cảnh, `useMotionPolicy` cho cảnh |

## 6. Test bắt buộc

- `geometry`: A* tránh vật cản, không đi chéo cắt góc, đích bị chặn trả `null`, đường kéo thẳng vẫn hợp lệ.
- `sim`:
  - agent làm việc đi về đúng bàn của mình và ngồi; bàn ổn định khi mood đổi; bàn được trả khi agent bị xoá;
  - agent rảnh đi dạo; hai agent rảnh có lúc tán gẫu (rng gieo hạt), bong bóng luân phiên;
  - đang tán gẫu mà có việc thì về bàn, người kia không kẹt ở trạng thái chờ;
  - hết `WANDER_MS` thì ngủ và `nextWakeAt` = `Infinity` (không timer);
  - `motion=false`: nhân vật đặt thẳng ở đích, không mốc nào;
  - hơn 16 agent: chỉ 16 trên sàn.
- Cảnh: render nhân vật có `aria-label` tên + trạng thái; agent giơ tay bấm/Enter được → `onSelect`; cảnh rỗng có
  gợi ý; Office giữ mọi test cũ (thẻ bàn, chọn thẻ duyệt, timer dừng khi ẩn và khi rảnh).
- `styles.test.ts` xanh (không màu/thời lượng viết cứng trong CSS).

## 7. Xác minh (2026-10-29)

- `pnpm -C ui test`: 231 test xanh (thêm `live/geometry.test.ts` 7, `live/sim.test.ts` 11, `scenes.test.tsx` 5 cho cảnh
  sống); `pnpm -C ui typecheck`, `pnpm -C ui build` sạch. Bundle chính 316 kB (không thêm thư viện).
- Chromium (Playwright) với trang demo tạm (agent giả, không commit): mở văn phòng thì người làm việc ngồi sẵn ở bàn, người
  rảnh đi dạo; sau ~5–10 s có cặp tán gẫu với bong bóng luân phiên; đổi mood sang `working` thì nhân vật nói "Có việc rồi!"
  và chạy về đúng bàn của mình; `done` thì ăn mừng tại bàn rồi đi giải lao; `approval` đứng giơ tay với "!", bấm "!" chọn
  đúng thẻ duyệt (focus `req_1`). Rộng 390 px: sàn co theo bề ngang, không tràn ngang. Không lỗi console.
- Chưa đo RAM/CPU trên Tauri Windows thật; các con số ngân sách ở §4 là thiết kế, cần `budget-guard`.

## 8. Chưa làm

- Âm thanh văn phòng, ngày/đêm động theo thời gian thực (cửa sổ chỉ lấy giờ lúc render), nhân vật VRM trong văn phòng.
- Đo RAM/CPU trên Tauri Windows thật (`budget-guard`), ghi hàng riêng vào BUDGET.md.
