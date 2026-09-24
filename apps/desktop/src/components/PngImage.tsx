import { useState } from "react";
import type { CSSProperties } from "react";
import { fallbackUrl } from "../lib/technologyIcons";

type Props = { src: string; alt: string; size?: number; variant?: "icon" | "logo" | "brand"; fallback?: string; className?: string; style?: CSSProperties };
export default function PngImage(props: Props) {
  return <ImageContent key={`${props.src}:${props.fallback}`} {...props}/>;
}
function ImageContent({src, alt, size = 24, variant = "icon", fallback = fallbackUrl(), className = "", style}: Props) {
  const [failed, setFailed] = useState(0);
  const [loaded, setLoaded] = useState(false);
  return <img src={failed === 0 ? src : fallback} alt={alt} width={size} height={size} className={`png-image ${className}`} data-variant={variant} data-loading={!loaded} style={{width: size, height: size, ...style}} onLoad={() => setLoaded(true)} onError={() => { setLoaded(true); if (failed === 0) setFailed(1); else setFailed(2); }} data-failed={failed === 2}/>;
}
