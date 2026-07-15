import { Moon, Sun } from "lucide-react";
import { Button } from "@/shared/ui/button";
import { useTheme } from "@/shared/theme/theme-provider";

/** Header control that flips between dark and light themes. */
export function ThemeToggle() {
  const { theme, toggle } = useTheme();
  const isDark = theme === "dark";
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      onClick={toggle}
      aria-label={isDark ? "Switch to light theme" : "Switch to dark theme"}
    >
      {isDark ? <Sun className="size-4" /> : <Moon className="size-4" />}
    </Button>
  );
}
