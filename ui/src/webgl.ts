// Kiểm máy có WebGL không (dùng chung cho cửa sổ pet và cảnh tòa nhà). Lỗi bất kỳ coi như không có. Context thử được trả lại
// ngay (`WEBGL_lose_context`) để không chiếm một trong số context ít ỏi của trình duyệt.
export function hasWebGL(): boolean {
  try {
    const c = document.createElement('canvas');
    const gl = c.getContext('webgl2') ?? c.getContext('webgl');
    if (gl === null) return false;
    gl.getExtension('WEBGL_lose_context')?.loseContext();
    return true;
  } catch {
    return false;
  }
}
