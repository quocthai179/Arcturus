// Đọc lời pet bằng giọng có sẵn của hệ điều hành (P9, tùy chọn, tắt mặc định). Web Speech `speechSynthesis` xử lý cục bộ;
// không ghi âm, không gửi gì ra ngoài. Chỉ đọc câu mẫu của pet, không đọc lệnh chờ duyệt.
import { useCallback, useEffect, useRef, useState } from 'react';

function synth(): SpeechSynthesis | null {
  return typeof window !== 'undefined' && 'speechSynthesis' in window ? window.speechSynthesis : null;
}

/** Giọng tiếng Việt nếu máy có, nếu không thì giọng mặc định. */
function pickVoice(s: SpeechSynthesis): SpeechSynthesisVoice | null {
  const voices = s.getVoices();
  return voices.find((v) => v.lang.toLowerCase().startsWith('vi')) ?? null;
}

export function voiceAvailable(): boolean {
  return synth() !== null;
}

export function useVoice(enabled: boolean) {
  const [speaking, setSpeaking] = useState(false);
  const current = useRef<SpeechSynthesisUtterance | null>(null);

  useEffect(() => {
    const s = synth();
    if (!enabled && s !== null) {
      s.cancel();
      setSpeaking(false);
    }
    return () => {
      s?.cancel();
    };
  }, [enabled]);

  const speak = useCallback(
    (text: string) => {
      const s = synth();
      if (!enabled || s === null || text.trim() === '') return;
      s.cancel();
      const u = new SpeechSynthesisUtterance(text);
      const v = pickVoice(s);
      if (v !== null) u.voice = v;
      u.lang = v?.lang ?? 'vi-VN';
      u.rate = 1.05;
      u.onstart = () => setSpeaking(true);
      u.onend = () => {
        if (current.current === u) setSpeaking(false);
      };
      u.onerror = u.onend;
      current.current = u;
      s.speak(u);
    },
    [enabled],
  );

  return { speak, speaking };
}
