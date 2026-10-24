import type { NextConfig } from "next";

const securityHeaders = [
  { key: "X-Content-Type-Options", value: "nosniff" },
  { key: "X-Frame-Options", value: "DENY" },
  { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
  { key: "Permissions-Policy", value: "camera=(), microphone=(), geolocation=()" },
  // Two years, subdomains included. Add `preload` only once the domain is submitted to hstspreload.org.
  { key: "Strict-Transport-Security", value: "max-age=63072000; includeSubDomains" },
];

const nextConfig: NextConfig = {
  // Cache Components and Partial Prefetching must be enabled together.
  // Both become the default in the next major release, so remove them then.
  cacheComponents: true,
  partialPrefetching: true,
  reactCompiler: true,
  typedRoutes: true,
  poweredByHeader: false,
  // Not `async`: Biome's `useAwait` rule rejects an async function without `await`.
  headers: () => Promise.resolve([{ source: "/:path*", headers: securityHeaders }]),
  // Tailwind CSS v4 loader for Turbopack (the default bundler). Builds with `--webpack` would skip it.
  turbopack: {
    rules: {
      "*.css": {
        loaders: ["@tailwindcss/turbopack"],
        as: "*.css",
      },
    },
  },
};

export default nextConfig;
