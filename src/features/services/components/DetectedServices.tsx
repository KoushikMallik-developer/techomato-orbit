import { Card } from '@/shared/ui/Card';
import type { Found } from '../api/useDetected';

/** Databases that are running but that Orbit could not take over (another account runs them, or
 *  Orbit cannot read how they were started). The rest already show as ordinary service cards. */
export function DetectedServices({ skipped }: { skipped: Found[] }) {
  if (skipped.length === 0) return null;
  return (
    <Card className="flex flex-col gap-1 text-sm">
      <h2 className="text-lg font-bold">Running, but Orbit cannot control these</h2>
      <ul className="text-fg-muted">
        {skipped.map((f) => (
          <li key={f.pid}>
            {f.name} on port {f.port}: another account runs it, or Orbit cannot read how it was
            started.
          </li>
        ))}
      </ul>
    </Card>
  );
}
