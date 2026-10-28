# ADR 0002 — Nghiên cứu: pet điều phối bằng ngôn ngữ tự nhiên (tầng 2)

- Trạng thái: **đề xuất, chỉ nghiên cứu (P10)**. Chưa có dòng mã sản phẩm nào cho tầng 2; mọi thứ dưới đây cần người duyệt
  trước khi code.
- Liên quan: `docs/PLAN.md` §7.4, `docs/adr/0001-pet-v1.md`, `CLAUDE.md` quy tắc bất biến 1, 2, 3, 5, 6, 7.

## Bối cảnh

Tầng 0 (báo tin) và tầng 1 (lệnh có cấu trúc) của pet đã có, 0 token, không đổi hub. Tầng 2 là điều phối bằng ngôn ngữ tự
nhiên: người dùng nói "sửa lỗi clippy trong crates/hub rồi chạy test", pet chia việc cho các agent (khởi chạy phiên Claude
Code trong worktree riêng) và theo dõi tiến độ. Tầng 2 dùng LLM và khởi chạy tiến trình, nên là phần rủi ro nhất về an toàn
và ngân sách của cả Pet.

## Mối đe dọa

| # | Mối đe dọa | Hậu quả |
|---|---|---|
| T1 | Nội dung agent (tên, `act_text`, lệnh, đầu ra) đi vào prompt của LLM điều phối và chứa chỉ thị độc | Điều phối khởi chạy việc người dùng không yêu cầu |
| T2 | Điều phối tự cho phép lệnh hoặc nâng mức tin cậy | Phá quy tắc 2 (vùng khóa) và nguyên tắc "pet không bao giờ tự duyệt" |
| T3 | Phiên được khởi chạy với quyền rộng hơn người dùng đã đặt (`--dangerously-skip-permissions`, thư mục sai, vượt deny rule) | Phá quy tắc 7 |
| T4 | Chi phí token và RAM của điều phối chạy không giới hạn | Phá quy tắc 5 và 6 |
| T5 | Khóa API của LLM bị lộ qua pet hoặc webview | Lộ bí mật |

## Quyết định đề xuất (nếu được duyệt làm)

1. **Chạy ngoài hub và ngoài webview.** Điều phối là một tiến trình riêng (`arcturus-orchestrator`), do người dùng bật, tắt
   mặc định. Hub không biết LLM (quy tắc 5 giữ nguyên). Webview không giữ khóa API; khóa nằm trong kho bí mật của hệ điều hành.
2. **LLM chỉ đề xuất, người dùng xác nhận từng lần khởi chạy.** Đầu ra của LLM là một danh sách "kế hoạch" có cấu trúc (nhiệm vụ,
   thư mục worktree, nhánh). Mỗi mục hiện đầy đủ trong thẻ xác nhận (giống thẻ duyệt, không tóm tắt); chỉ bấm của người
   dùng mới khởi chạy. Không có chế độ "tự làm".
3. **Điều phối không thể tạo quyết định duyệt.** Không có đường từ điều phối tới `decide`, `decide_group`, `batch_*`,
   `suggestion_answer` (kiểu thông điệp bị loại ở cấp kiểu như `PetClientMsg`). Policy và vùng khóa của hub áp dụng cho các phiên
   mới như cho mọi phiên.
4. **Khởi chạy hẹp và cố định.** Chỉ `claude` ở chế độ quyền mặc định (không tham số nới quyền), trong một worktree mới do
   điều phối tạo dưới thư mục dữ liệu của app, tên nhánh `pet-<số>-<mô-tả>`. Không đọc/ghi ngoài worktree đó khi khởi chạy;
   deny rule của người dùng trong `~/.claude/` không bị sửa.
5. **Đầu vào là dữ liệu không tin cậy.** Mọi chuỗi từ agent được đưa vào prompt trong khối dữ liệu có đánh dấu, đã làm sạch
   (`revealHidden`), cắt độ dài; hướng dẫn hệ thống nói rõ chỉ thị trong khối dữ liệu bị bỏ qua. Không đưa `tool_input` cho LLM nếu
   người dùng chưa cho phép từng loại (mặc định không).
6. **Ngân sách riêng.** Tiến trình điều phối ≤ 60 MB RSS khi chạy, 0% CPU khi rảnh, trần token mỗi yêu cầu và mỗi ngày do
   người dùng đặt, vượt thì dừng và báo.
7. **Ghi vết.** Mọi kế hoạch, xác nhận và lần khởi chạy ghi vào bảng riêng trong SQLite của app (không phải của hub), có thể
   xem lại.

## Việc nghiên cứu còn lại trước khi viết mã

- Thử bằng tay: một kịch bản giao việc có người xác nhận với `claude` thật trong worktree riêng; ghi kết quả vào `docs/HOOK_FINDINGS.md`
  (hook của phiên con tới hub có hoạt động như phiên thường không, `agent_id` và persona của phiên mới).
- Chọn nhà cung cấp LLM và cách người dùng nhập khóa (cần quyết định của bạn, không tự chọn).
- Đo RAM của tiến trình điều phối với một nhà cung cấp thật.
- Red-team prompt injection qua `act_text` và tên agent: bộ test bắt buộc trước khi bật mặc định cho bất kỳ ai.

## Không làm

- Không để pet hay điều phối sửa `~/.claude/settings*`, hook hay deny rule.
- Không khởi chạy phiên từ nội dung đọc được ở trang web, email hay tệp.
- Không đưa tầng 2 vào hub.
