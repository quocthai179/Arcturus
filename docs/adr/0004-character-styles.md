# ADR 0004 — Kiểu nhân vật 2D: Chibi Plus (mặc định) và Pixel Sprite (tùy chọn)

- Trạng thái: **chấp nhận hướng đi** (người dùng chọn A và C, 2026-11-11). Chưa có mã ARC; triển khai theo
  `docs/CHARACTER_STYLES.md` §10.
- Phạm vi: `ui/` (làn `ui`). Hub, schema, giao thức, `crates/app/` không đổi.
- Liên quan: `docs/UI_EXTENSION.md` (`AVATAR_RENDERERS`), `docs/OFFICE_LIVE.md`, `docs/PLAN.md` T11.

## Bối cảnh

Nhân vật 2D hiện tại (T11) là nửa thân, mood thể hiện chủ yếu bằng mắt và miệng. Người dùng thấy quá đơn giản: khó phân
biệt các agent, tư thế không cho biết ai đang làm gì. Ba mẫu đã được vẽ (`docs/mockups/character-styles.html` giữ lại A
và C):

- **A · Chibi Plus**: toàn thân, đầu to, 6 kiểu tóc, phụ kiện, mood thể hiện bằng cả tư thế và gương mặt. ≈ 47 nút SVG.
- **B · Flat Editorial**: tỉ lệ người lớn, khối màu phẳng, mặt tối giản, mood qua đồ vật. ≈ 26 nút SVG.
- **C · Pixel Sprite**: sprite 16 ô có viền, dữ liệu là lưới ký tự. ≈ 98–124 nút SVG.

## Quyết định

1. **A là kiểu mặc định** thay kiểu `svg` hiện tại. **C là kiểu tùy chọn.** **B bị loại.**
2. Cả hai đi qua `AVATAR_RENDERERS`. `AvatarRendererProps` thêm `seed` (id agent, chỉ để băm) và `pose`
   (`stand | sit | walk`), cả hai tùy chọn, tương thích ngược.
3. Ngoại hình (tóc, màu da, phụ kiện) chọn tất định từ id agent bằng một mô-đun thuần dùng chung (`lookOf`), nên cùng một
   agent trông giống nhau ở mọi cửa sổ và mọi kiểu.
4. Mọi nơi hiện nhân vật 2D (thẻ duyệt, bàn, đội, popup, pet 2D, văn phòng sống) đi qua `AgentAvatar`. `Walker` của văn
   phòng sống bỏ chân tự vẽ, dùng `pose`.
5. Kiểu được chọn lưu ở `localStorage` của UI; không thêm cấu hình vào hub.
6. Kiểu cũ đổi tên `classic`, giữ một bản phát hành rồi gỡ.

## Phương án đã cân nhắc

| Phương án | Lý do không chọn |
|---|---|
| Chỉ A | Người dùng muốn giữ cả cảm giác game của C |
| Chỉ C | Đổi giọng điệu toàn app; mặt pixel khó biểu cảm ở thẻ duyệt cỡ lớn |
| B | Mặt quá tối giản, agent khó phân biệt, đúng điều người dùng phàn nàn |
| Ảnh PNG/sprite sheet vẽ tay | Cần tài sản ngoài và giấy phép; không đổi màu theo persona được |
| Lottie hoặc canvas cho mọi kiểu | Thêm phụ thuộc hoặc mất khả năng kiểm bằng DOM; chỉ dùng canvas cho C nếu số đo yêu cầu |

## Hệ quả

- Nhân vật A có nhiều nút SVG hơn kiểu cũ; C nhiều nhất. Văn phòng sống tối đa 16 nhân vật, nên cần đo lại ngân sách
  (`/budget-check`) trước khi coi C là ổn. Có sẵn đường lui: vẽ C ra canvas một lần mỗi tổ hợp.
- `Walker` và các nơi đang gọi `Character` trực tiếp phải sửa một lần; sau đó đổi kiểu không phải sửa nơi dùng.
- Cảnh 3D (`building`) không bị ảnh hưởng; nhân vật 3D có hệ ngoại hình riêng (`ui/src/building/figure.ts`).

## Bất biến

- Bộ vẽ chỉ vẽ: không gọi hub, không đọc store, không timer, không quyết định gì về duyệt.
- Không vẽ chuỗi do agent cung cấp vào SVG; tên agent ở nhãn React bên ngoài. `seed` chỉ được băm.
- Màu ngoài `#RRGGBB` bị thay bằng màu mặc định.
- Chuyển động chỉ khi được phép (`useMotionPolicy`, `prefers-reduced-motion`, trang hiện); không animation vô hạn khi rảnh.
