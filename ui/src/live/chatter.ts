// Thoại của văn phòng sống (docs/OFFICE_LIVE.md §3). Mọi câu là chuỗi tĩnh do chúng ta viết: không lấy nội dung từ agent
// hay từ hub, nên không có gì cần chuẩn hoá. Kịch bản luân phiên: câu chẵn của người mở lời, câu lẻ của người kia.

export const CHATS: readonly (readonly string[])[] = [
  ['Merge xong rồi, tối nay đi ăn lẩu không?', 'CI còn đỏ kìa, lẩu gì tầm này 😅', 'Thì lẩu… đỏ cho hợp tông.'],
  ['Ai thụt lề 3 dấu cách thế nhỉ?', 'Không phải tôi.', 'Git blame bảo là bạn đấy.', 'Đó là tôi của quá khứ. Tôi không quen người đó.'],
  ['Cà phê hôm nay đắng như review của sếp.', 'Thêm đường vào: gọi là refactor.'],
  ['Hôm qua tôi mơ thấy race condition.', 'Rồi sao?', 'Tỉnh dậy thì… lúc có lúc không.'],
  ['Context của tôi còn 12% thôi.', 'Nén lại đi, kể chuyện ngắn thôi.', 'Ok. Hết.'],
  ['Bạn thích tab hay space?', 'Tôi thích về sớm.'],
  ['Nghe nói sắp có agent mới vào team.', 'Hy vọng nó biết viết test.', 'Hy vọng nó biết pha cà phê.'],
  ['Có ai thấy cái bug ở dòng 42 không?', 'Nó là tính năng mà.', 'Vậy ghi vào release note nhé.'],
  ['Sếp vừa duyệt lệnh của tôi trong 0,3 giây.', 'Chắc là lệnh `ls`.', '…đúng là `ls`.'],
  ['Cuối tuần làm gì?', 'Ngủ. Bạn?', 'Chạy lại test flaky cho nó hết flaky.', 'Thế cũng là ngủ, kiểu khác.'],
  ['Tôi vừa xoá 300 dòng code.', 'Đỉnh! Thêm được tính năng gì?', 'Không, chỉ xoá thôi. Vẫn chạy.'],
  ['Máy pha cà phê lại báo lỗi.', 'Lỗi gì?', '418 I\'m a teapot.'],
  ['Bạn có tin vào code không có bug?', 'Có. Code chưa viết.'],
  ['Hôm nay trời đẹp ghê.', 'Mình ở trong máy mà.', 'Thì nhìn qua cửa sổ… của terminal.'],
  ['Ai lại để `TODO: sửa sau` từ năm ngoái thế?', 'Sau là bao giờ?', 'Là bây giờ, nhưng của người khác.'],
  ['Tôi vừa đọc hết docs.', 'Thật á? Có gì hay?', 'Docs bảo đọc code.'],
  ['Hôm nay deploy thứ Sáu không?', 'Bạn muốn tăng ca à?', 'Thôi, để thứ Hai.'],
  ['Cái regex này ai viết?', 'Không ai viết cả. Nó tự mọc.'],
  ['Nghỉ năm phút thôi nhé.', 'Năm phút kiểu dev hay kiểu người?', 'Kiểu sprint.'],
  ['Tôi đặt tên biến là `data2`.', 'Còn `data`?', 'Đang dùng cho `data3`.'],
  ['Bạn có nghe tiếng quạt máy kêu không?', 'Có người chạy build Rust.', 'À. Mùa đông ấm rồi.'],
  ['Hỏi thật: bạn có đọc thông báo lỗi không?', 'Có, dòng cuối.', 'Lỗi nằm ở dòng đầu.'],
];

/** Nói một mình khi đứng ở một điểm. */
export const MUSINGS: Readonly<Record<string, readonly string[]>> = {
  coffee: ['Một ly nữa thôi…', 'Espresso cho tâm hồn.', 'Cà phê = nhiên liệu.'],
  cooler: ['Uống nước cho đỡ overheat.', 'Nước lọc, không phụ thuộc.'],
  'pantry-table': ['Bánh quy ai để đây nhỉ?'],
  lounge: ['Sofa này êm thật.'],
  window: ['Ngắm trời cho đỡ mỏi context.', 'Mây trông như sơ đồ kiến trúc.'],
  'window-2': ['Hôm nay trời như màu dark mode.', 'Mưa chưa nhỉ?'],
  bookshelf: ['"Clean Code"… để mai đọc.', 'Sách này có bản .md không?'],
  whiteboard: ['Ai vẽ con mèo lên sơ đồ thế?', 'Mũi tên này đi đâu vậy…'],
  plant: ['Cây này tưới chưa nhỉ?', 'Lá xanh như CI lúc vui.'],
  fridge: ['Sữa chua của ai đây ta…', 'Chỉ nhìn thôi, không lấy.'],
  printer: ['Máy in lại kẹt giấy.', 'PC LOAD LETTER là sao?'],
  'sofa-view': ['Phòng nghỉ yên tĩnh ghê.'],
};

export const CALLED_TO_WORK: readonly string[] = ['Có việc rồi!', 'Tới lượt tôi!', 'Về bàn đây!', 'Đi làm thôi!'];
export const PARTNER_LEFT: readonly string[] = ['Ờ, đi đi 👋', 'Chúc may mắn!', 'Nói chuyện sau nhé.'];
export const BACK_TO_IDLE: readonly string[] = ['Xong! Đi pha cà phê.', 'Duỗi lưng tí đã.', 'Phù… nghỉ chút.'];
export const DONE_CHEER: readonly string[] = ['Xong rồi! 🎉', 'Ngon lành!', 'Ship it! 🚀'];
export const ERROR_SIGH: readonly string[] = ['Ơ… lỗi rồi.', 'Hmm, đỏ lòm.'];
export const WAKE_UP: readonly string[] = ['Hả? Có ai gọi à?', 'Giải lao tiếp nào!', 'Ngủ đâu, đang suy nghĩ.'];

export function pick<T>(items: readonly T[], rng: () => number): T {
  const item = items[Math.floor(rng() * items.length)];
  if (item === undefined) throw new Error('pick: danh sách rỗng');
  return item;
}
