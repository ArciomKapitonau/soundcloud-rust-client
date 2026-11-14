import { useState } from "react";
import { Music2 } from "lucide-react";

export function Cover({ src, alt, className = "", rounded = "rounded-sm" }: { src: string | null; alt: string; className?: string; rounded?: string }) {
  const [failed, setFailed] = useState(false);
  if (!src || failed) {
    return (
      <div className={`flex items-center justify-center bg-gradient-to-br from-accent-soft to-surface-2 text-accent/70 ${rounded} ${className}`}>
        <Music2 className="h-1/3 w-1/3" />
      </div>
    );
  }
  return (
    <img
      src={src}
      alt={alt}
      loading="lazy"
      draggable={false}
      onError={() => setFailed(true)}
      className={`object-cover bg-surface-2 ${rounded} ${className}`}
    />
  );
}
