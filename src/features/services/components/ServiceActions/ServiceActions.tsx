import { Button } from '@/shared/ui/Button';
import type { ServiceStatus } from '@/shared/ui/StatusDot/status';
import { useServiceActions } from '../../api/useRuntime';
import { availableActions } from './powerState';

export function ServiceActions({ id, status }: { id: string; status: ServiceStatus }) {
  const act = useServiceActions();
  const can = availableActions(status);
  return (
    <div className="flex flex-wrap gap-2">
      <Button
        size="sm"
        variant="primary"
        disabled={!can.start || act.start.isPending}
        onClick={() => act.start.mutate(id)}
      >
        Start
      </Button>
      <Button
        size="sm"
        disabled={!can.stop || act.stop.isPending}
        onClick={() => act.stop.mutate(id)}
      >
        Stop
      </Button>
      <Button
        size="sm"
        disabled={!can.restart || act.restart.isPending}
        onClick={() => act.restart.mutate(id)}
      >
        Restart
      </Button>
      <Button
        size="sm"
        variant="danger"
        disabled={!can.forceStop || act.forceStop.isPending}
        onClick={() => act.forceStop.mutate(id)}
      >
        Force stop
      </Button>
    </div>
  );
}
