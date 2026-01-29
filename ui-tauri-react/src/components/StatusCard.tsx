import type { ReactNode } from "react";

interface StatusCardProps {
  title: string;
  icon?: ReactNode;
  children: ReactNode;
  className?: string;
}

export default function StatusCard({ title, icon, children, className }: StatusCardProps) {
  return (
    <div className={`glass-card rounded-xl p-5 ${className ?? ""}`}>
      <div className="mb-4 flex items-center gap-2">
        {icon && (
          <span className="text-primary/80">{icon}</span>
        )}
        <h3 className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
          {title}
        </h3>
      </div>
      {children}
    </div>
  );
}
