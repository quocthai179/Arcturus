# Văn phòng 2D — T13 / ARC-34

Mở từ menu tray **Mở văn phòng**. Trong môi trường phát triển UI, dùng `?view=office` trên Vite dev server.
Office dùng cùng cơ chế xác thực hub với popup Lite; không cần endpoint hay giao thức mới.

Từ 2026-10-29 cảnh mặc định là **văn phòng sống** (`live`, docs/OFFICE_LIVE.md): agent rảnh đi dạo, tán gẫu; có việc thì
chạy về bàn; lưới thẻ dưới đây vẫn hiện ngay dưới sàn. Công tắc "Sống / Lưới" ở đầu Office chọn cảnh (nhớ ở `localStorage`).

## Hiển thị

- Mỗi bàn có nhân vật SVG, tên, vai trò, mood, hoạt động, nguồn, máy và context nếu hub cung cấp.
  `context_pct = null` hiện “chưa có dữ liệu”, không coi là 0%.
- Thứ tự: giơ tay → lỗi → đang làm → xong → rảnh. Trong nhóm, sự kiện mới nhất trước; trùng thời gian thì theo id.
- `team.roster_max` giới hạn phần đội hình ưu tiên, mặc định 5. Chỉ agent **rảnh** vượt giới hạn được gom vào
  “Phòng hỗ trợ”. Agent không rảnh luôn có bàn riêng, kể cả khi vượt giới hạn. Agent con cũng được hiển thị.
- Bấm tên agent đang giơ tay (hoặc dùng Tab + Enter) chọn yêu cầu cũ nhất còn chờ, cuộn và chuyển focus đến thẻ.
  Nếu chỉ có handoff, chuyển đến thẻ “Duyệt trong terminal”. Khi yêu cầu biến mất, bàn không còn nút chọn đó.
- Cột phải dùng lại QueueList: cùng điều kiện duyệt, thời gian xem tối thiểu, xem đầy đủ nội dung bị cắt,
  cảnh báo và các thẻ handoff. Màn hình hẹp chuyển thành một cột.

## Hiệu năng và vòng đời

Không có requestAnimationFrame thường trực. Đồng hồ UI chỉ chạy khi có yêu cầu chờ và trang đang hiện.
Khi Page Visibility chuyển sang hidden, timer dừng và Office tắt CSS animation/transition, kể cả hiệu ứng Character.
Unmount đóng WebSocket, hủy timer và listener; mở lại nhận snapshot mới từ hub.
Mất kết nối làm khóa Allow; UI vẫn thể hiện thông tin gần nhất cùng thông báo lỗi.

Đóng cửa sổ Office sử dụng vòng đời đóng mặc định của Tauri hiện có; không bổ sung cơ chế hide giữ webview.
Không thay đổi luồng tray, hub hoặc giao thức. Nút điều khiển đội và countdown phủ quyết thuộc T14.

## Xác minh ngày 2026-10-26

- Vitest kiểm tra sắp xếp, gom agent rảnh, agent con, đổi giới hạn/trạng thái, context, chọn thẻ/handoff,
  điều kiện Allow/Deny, vòng đời timer, snapshot/delta/reconnect và đóng socket. Chạy từ repo:
  `pnpm -C ui test --run` và `pnpm -C ui build`.
- Chromium + hub debug thật, SQLite và token riêng: 7 agent thử, giới hạn 3; 4 agent không rảnh vẫn có bàn,
  3 agent rảnh vào phòng hỗ trợ. Gửi PermissionRequest rồi bấm Allow/Deny trả đúng JSON quyết định cho hook.
  Không chạy các lệnh hiển thị trên thẻ. Handoff, xóa agent và layout rộng 390px không tràn ngang đều được kiểm tra.
- Chromium DevTools trace 1,2 giây sau khi **mô phỏng** `document.hidden = true` và phát `visibilitychange`,
  trong khi hub thật còn giữ một yêu cầu: 0 Paint, 0 FireAnimationFrame, 0 TimerFire.
  Đây là phép kiểm đường xử lý Page Visibility, không phải phép đo cửa sổ Tauri thực sự bị thu nhỏ/che khuất.
- Chưa đo CPU/RSS Office, kiểm tra mở/đóng webview và tab ẩn trên Tauri/Windows thật. Các mục tiêu tài nguyên
  trong BUDGET.md vẫn là mục tiêu, không phải số đo đạt được từ kiểm tra trình duyệt này.
