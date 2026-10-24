import type { ComponentProps } from "react";

const variants = {
  primary:
    "bg-paper font-semibold text-ink transition-[background-color,box-shadow] duration-350 hover:bg-white hover:shadow-[0_0_0_5px_rgb(236_231_218/0.1),0_16px_40px_rgb(0_0_0/0.35)]",
  secondary:
    "border border-paper/40 transition-[border-color,background-color] duration-350 hover:border-paper hover:bg-paper/7",
} as const;

const sizes = {
  md: "min-h-10 px-4 text-[0.9rem]",
  lg: "min-h-[3.125rem] px-6 text-base",
} as const;

type ButtonLinkProps = ComponentProps<"a"> & {
  variant?: keyof typeof variants;
  size?: keyof typeof sizes;
  external?: boolean;
};

export function ButtonLink({
  variant = "primary",
  size = "lg",
  external = false,
  className = "",
  ...props
}: ButtonLinkProps) {
  return (
    <a
      {...(external ? { target: "_blank", rel: "noopener noreferrer" } : {})}
      {...props}
      className={`inline-flex items-center justify-center gap-2.5 rounded-xs active:translate-y-px ${variants[variant]} ${sizes[size]} ${className}`}
    />
  );
}
