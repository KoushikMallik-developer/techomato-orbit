import { useMemo, useState } from 'react';
import { ShieldCheck } from 'lucide-react';
import { Card } from '@/shared/ui/Card';
import { EmptyState } from '@/shared/ui/EmptyState';
import { SearchInput, Select } from '@/shared/ui/Field';
import { usePins, useWinControl, useWinServices } from '../api/useWinServices';
import { WinServiceTable } from '../components/WinServiceTable';
import { filterWinServices, type StartFilter, type StateFilter } from '../winFilter';

export function WindowsServicesSection() {
  const list = useWinServices();
  const { pins, setPins } = usePins();
  const control = useWinControl();
  const [query, setQuery] = useState('');
  const [state, setState] = useState<StateFilter>('all');
  const [start, setStart] = useState<StartFilter>('all');
  const [showAll, setShowAll] = useState(false);
  const rows = useMemo(
    () => filterWinServices(list.data ?? [], query, state, start, showAll),
    [list.data, query, state, start, showAll],
  );
  const isPinned = (n: string) => pins.some((p) => p.toLowerCase() === n.toLowerCase());

  return (
    <section className="flex flex-col gap-3" aria-labelledby="win-services-h">
      <h2 id="win-services-h" className="text-xl font-extrabold">
        Windows services
      </h2>
      <div className="flex flex-wrap items-center gap-3">
        <SearchInput
          aria-label="Search Windows services"
          placeholder="Search name or PID"
          className="w-72"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <Select
          aria-label="State"
          value={state}
          onChange={(e) => setState(e.target.value as StateFilter)}
        >
          <option value="all">Any state</option>
          <option value="running">Running</option>
          <option value="stopped">Stopped</option>
        </Select>
        <Select
          aria-label="Start type"
          value={start}
          onChange={(e) => setStart(e.target.value as StartFilter)}
        >
          <option value="all">Any start type</option>
          <option value="auto">Automatic</option>
          <option value="autoDelayed">Automatic (delayed)</option>
          <option value="manual">Manual</option>
          <option value="disabled">Disabled</option>
        </Select>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={showAll} onChange={(e) => setShowAll(e.target.checked)} />
          Show all Windows services
        </label>
      </div>
      {list.isError && (
        <Card role="alert" className="text-danger">
          Could not read Windows services: {list.error.message}
        </Card>
      )}
      {list.data && rows.length === 0 && (
        <EmptyState
          icon={ShieldCheck}
          title="No matching services"
          hint={
            showAll
              ? 'Nothing fits the current filters.'
              : 'No development services found. Turn on Show all and pin the ones you use.'
          }
        />
      )}
      {rows.length > 0 && (
        <WinServiceTable
          rows={rows}
          pins={pins}
          busy={control.isPending}
          onControl={(name, action) => control.mutate({ name, action })}
          onTogglePin={(name) =>
            setPins(
              isPinned(name)
                ? pins.filter((p) => p.toLowerCase() !== name.toLowerCase())
                : [...pins, name],
            )
          }
        />
      )}
    </section>
  );
}
