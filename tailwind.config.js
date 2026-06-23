/** @type {import('tailwindcss').Config} */
export default {
  // 跟随系统浅深色（不引入主题切换器，保持原有行为）
  darkMode: "media",
  content: ["./index.html", "./capsule.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      borderRadius: {
        lg: "var(--radius)",
        md: "calc(var(--radius) - 2px)",
        sm: "calc(var(--radius) - 4px)",
      },
      colors: {
        // shadcn 语义色 → 映射到我们的 macOS token（见 src/index.css）
        border: "var(--border)",
        input: "var(--control-border)",
        ring: "var(--accent)",
        background: "var(--bg)",
        foreground: "var(--fg)",
        primary: {
          DEFAULT: "var(--accent)",
          foreground: "var(--accent-fg)",
        },
        secondary: {
          DEFAULT: "var(--control)",
          foreground: "var(--fg)",
        },
        muted: {
          DEFAULT: "var(--surface-hover)",
          foreground: "var(--fg-2)",
        },
        accent: {
          DEFAULT: "var(--surface-hover)",
          foreground: "var(--fg)",
        },
        popover: {
          DEFAULT: "var(--surface)",
          foreground: "var(--fg)",
        },
        card: {
          DEFAULT: "var(--surface)",
          foreground: "var(--fg)",
        },
      },
    },
  },
  plugins: [],
};
