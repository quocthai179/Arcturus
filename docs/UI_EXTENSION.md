# Mở rộng giao diện (UI)

Mục đích: sau này thay đổi diện mạo (kiểu dáng đẹp hơn, nhân vật chuyển động, văn phòng động, 3D VRM) mà **không phải
viết lại logic duyệt** và **không phá ngân sách tài nguyên**. Tài liệu này ghi các điểm thay thế đã có sẵn và luật để
giữ chúng. Phạm vi v0 không đổi: chưa làm ảnh động nặng, 3D, giọng nói (CLAUDE.md).

## Các lớp

```
protocol.ts / store.ts / ws.ts      dữ liệu từ hub (nguồn sự thật, không có giao diện)
useApprovalQueue / useHubConnection logic duyệt và kết nối (hook, không có JSX)
countdown.ts / text.ts / office.ts  hàm thuần (đếm ngược, xử lý chữ, sắp xếp đội hình)
components/ views/                  trình bày: nhận dữ liệu + callback, trả JSX
index.css :root                     token kiểu dáng (màu, bo góc, khoảng cách, thời lượng)
```

Luật: lớp dưới không biết lớp trên. **Trình bày không chứa logic nghiệp vụ**: điều kiện được phép duyệt (`allowGate`),
đếm ngược, nhóm Critical không gộp, hiển thị `command_normalized` đầy đủ nằm ở hook/hàm thuần và có test. Đổi diện mạo
không được làm đổi các điều kiện đó (CLAUDE.md quy tắc bất biến 2 và 3).

## Điểm thay thế

| Muốn thay | Điểm cắm | Cách làm |
|---|---|---|
| Cách vẽ nhân vật (SVG chuyển động, Lottie, 3D VRM) | `components/AgentAvatar.tsx`: `AVATAR_RENDERERS` | Thêm khóa vào `AvatarStyle` và một component nhận `AvatarRendererProps` (`mood`, `palette`, `size`, `label`). Nơi dùng (thẻ duyệt, bàn, đội) không đổi. |
| Cảnh Văn phòng (lưới, cảnh 2D có bàn/ánh sáng, isometric, 3D) | `components/scenes.tsx`: `OFFICE_SCENES` | Thêm khóa vào `SceneId` và một component nhận `OfficeSceneProps` (`desks`, `support`, `canSelect`, `onSelect`, `motion`). `Office` không đổi. Đã có: `live` (mặc định, docs/OFFICE_LIVE.md) và `grid`. |
| Màu, bo góc, khoảng cách, thời lượng | `index.css` khối `:root` | Đổi giá trị token; chủ đề mới là một khối `:root[data-theme='...']` ghi đè token. |
| Quy tắc chuyển động | `useMotionPolicy.ts` + quy tắc `data-motion` trong `index.css` | Không sửa từng hiệu ứng; mọi hiệu ứng tự được tắt. |

Hai hợp đồng quan trọng:

- **Bộ vẽ nhân vật và cảnh chỉ vẽ.** Không gọi hub, không đọc store, không gửi lệnh duyệt. Chọn agent đang giơ tay thì cảnh
  gọi `onSelect(agentId)`.
- **Hàng đợi duyệt không phụ thuộc cảnh.** Thẻ duyệt luôn nằm ở cột riêng (`QueueList`), để cảnh 3D tải lỗi hay không có
  WebGL thì vẫn duyệt được. Cảnh mới phải tự lui về `grid` khi lỗi (sẽ làm cùng cảnh đầu tiên khác `grid`).

## Luật chuyển động và tài nguyên (mọi thay đổi giao diện phải giữ)

1. Chuyển động chỉ chạy khi trang đang hiện và người dùng không bật giảm chuyển động. `useMotionPolicy` (gắn ở `App`)
   đặt `data-motion="on|off"` lên `<html>`; khi `off`, `index.css` tắt mọi `animation`/`transition`. Mã JS (canvas, WebGL,
   WAAPI) đọc `motionAllowed`/giá trị trả về của hook, không tự nghe `visibilitychange`.
2. Không vòng lặp chạy mãi khi rảnh; hiệu ứng theo sự kiện (đổi trạng thái, có yêu cầu mới) và ngắn. Khi có hoạt động
   tối đa 15 FPS; không `requestAnimationFrame` thường trực (docs/BUDGET.md, T13).
3. Đóng cửa sổ thì hủy webview (popup) và giải phóng tài nguyên của bộ vẽ (texture, WebGL context).
4. Bộ vẽ nặng (three.js, VRM) nạp muộn bằng `React.lazy`/`import()`, không vào bundle chính. Đo RAM/GPU bằng
   `budget-guard` và ghi hàng riêng trong BUDGET.md trước khi bật mặc định.
5. Kiểu dáng chỉ dùng token. `styles.test.ts` fail nếu CSS (ngoài khối `:root`) có màu hex, `border-radius` hay thời lượng
   `animation`/`transition` viết cứng. Màu riêng của nhân vật (da, mực) nằm trong bộ vẽ SVG, không phải CSS.
6. Màu persona chỉ nhận `#RRGGBB` (hub kiểm ở `config.rs`, bộ vẽ kiểm lại) vì đi thẳng vào thuộc tính SVG.

## Danh sách kiểm cho PR đổi giao diện

- [ ] Chỉ chạm lớp trình bày hoặc token; không sửa `allowGate`, `countdown.ts`, `groupActions`, `store.ts`.
- [ ] Có đường lui khi bộ vẽ/cảnh mới lỗi hoặc máy không có WebGL.
- [ ] `pnpm -C ui test` và `tsc -b` xanh; `styles.test.ts` xanh.
- [ ] Đã đo (budget-guard) nếu thêm thư viện hoặc vòng vẽ; số đo ghi vào BUDGET.md.
- [ ] Có ảnh chụp trước/sau ở cả popup (380×560) và Office; kiểm bằng bàn phím và `prefers-reduced-motion`.

## Chưa làm (ghi nhận)

- Tùy chọn người dùng cho kiểu nhân vật/cảnh/chủ đề (hiện dùng mặc định `DEFAULT_AVATAR_STYLE`, `DEFAULT_SCENE`).
- Chuỗi chữ tiếng Việt viết thẳng trong component; khi cần đa ngôn ngữ thì gom vào một bảng chuỗi.
- Nhân vật SVG còn một số màu cố định (da, mực, miệng) trong `Character.tsx`.
