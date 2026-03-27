import type { StrengthLevel } from "../types/auth";

export function computePasswordStrength(val: string): StrengthLevel {
  if (!val) {
    return { width: "0%", color: "rgba(0, 212, 255, 1)", text: "" };
  }

  let score = 0;
  if (val.length >= 8) score++;
  if (/[A-Z]/.test(val)) score++;
  if (/[0-9]/.test(val)) score++;
  if (/[^A-Za-z0-9]/.test(val)) score++;

  const levels: StrengthLevel[] = [
    { width: "20%", color: "#ff4a6e", text: "WEAK" },
    { width: "45%", color: "#ff8c42", text: "FAIR" },
    { width: "70%", color: "#f9c74f", text: "GOOD" },
    { width: "100%", color: "#00d4ff", text: "STRONG" },
  ];

  const idx = Math.max(0, score - 1);
  return levels[idx] ?? levels[0]!;
}
