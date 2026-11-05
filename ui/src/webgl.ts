// Kiểm máy có WebGL không (dùng chung cho cửa sổ pet và cảnh tòa nhà). Lỗi bất kỳ coi như không có.
export function hasWebGL(): boolean {
  try {
    const c = document.createElement('canvas');
    return (c.getContext('webgl2') ?? c.getContext('webgl')) !== null;
  } catch {
    return false;
  }
}
