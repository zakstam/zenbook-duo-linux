import { useTheme } from "next-themes";
import { Button } from "@/components/ui/button";
import { IconMoon, IconSun } from "@tabler/icons-react";

export default function ThemeToggle() {
  const { resolvedTheme, setTheme } = useTheme();
  const isDark = resolvedTheme === "dark";

  return (
    <Button
      variant="ghost"
      size="sm"
      className="w-full justify-start gap-2.5 text-[13px] font-medium text-muted-foreground hover:text-foreground"
      onClick={() => setTheme(isDark ? "light" : "dark")}
    >
      <div className="relative flex size-[18px] items-center justify-center">
        {isDark ? (
          <IconMoon className="size-[18px]" stroke={1.5} />
        ) : (
          <IconSun className="size-[18px]" stroke={1.5} />
        )}
      </div>
      {isDark ? "Dark Mode" : "Light Mode"}
    </Button>
  );
}
