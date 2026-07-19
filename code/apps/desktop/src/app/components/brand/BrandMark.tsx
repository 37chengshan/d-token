import type { ImgHTMLAttributes } from "react";
import catIcon from "../../../assets/dtoken-cat-icon-96.png";

export function BrandMark({ className, alt = "d-token", ...props }: ImgHTMLAttributes<HTMLImageElement>) {
  return (
    <img
      src={catIcon}
      alt={alt}
      className={className}
      draggable={false}
      {...props}
    />
  );
}
