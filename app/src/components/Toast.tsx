import { useEffect, useRef, useState } from "react";
import { onToast } from "../toast";

/** 底部那行轻提示：出现 2 秒后自己消失，不打断操作。 */
export default function Toast() {
  const [message, setMessage] = useState("");
  const [seq, setSeq] = useState(0);
  const timer = useRef<number | undefined>(undefined);

  useEffect(
    () =>
      onToast((text) => {
        // seq 让同一条文字连发两次也能重新计时
        setMessage(text);
        setSeq((n) => n + 1);
      }),
    [],
  );

  useEffect(() => {
    if (!message) return;
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setMessage(""), 2000);
    return () => window.clearTimeout(timer.current);
  }, [message, seq]);

  if (!message) return null;
  return (
    <div className="toast" role="status" data-block="toast">
      {message}
    </div>
  );
}
