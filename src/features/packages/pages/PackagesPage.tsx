import { useMemo, useState } from 'react';
import { Package as PackageIcon, Search } from 'lucide-react';
import { useQueryClient } from '@tanstack/react-query';
import { Button } from '@/shared/ui/Button';
import { EmptyState } from '@/shared/ui/EmptyState';
import { SearchInput } from '@/shared/ui/Field';
import { PageHeader } from '@/shared/ui/PageHeader';
import { Segmented } from '@/shared/ui/Segmented';
import { MANAGER_LABEL, type Manager } from '../api/packages.schema';
import { useManagers, usePackageList, usePackageRun, type View } from '../api/usePackages';
import { OutputDrawer } from '../components/OutputDrawer';
import { PackageTable } from '../components/PackageTable';
import { filterPackages } from '../packageFilter';

export function PackagesPage() {
  const qc = useQueryClient();
  const managers = useManagers();
  const available = useMemo(
    () => (managers.data ?? []).filter((m) => m.available).map((m) => m.manager),
    [managers.data],
  );
  const [picked, setPicked] = useState<Manager | null>(null);
  const manager = picked && available.includes(picked) ? picked : available[0];
  const [view, setView] = useState<View>('installed');
  const [filter, setFilter] = useState('');
  const [draft, setDraft] = useState('');
  const [query, setQuery] = useState('');
  const list = usePackageList(manager, view, query);
  const { run, start, busy, close } = usePackageRun();

  const rows = useMemo(() => filterPackages(list.data ?? [], filter), [list.data, filter]);
  const refresh = () => qc.invalidateQueries({ queryKey: ['pm', 'list', manager, view] });

  if (managers.data && available.length === 0) {
    return (
      <div className="flex flex-col gap-4">
        <PageHeader title="Packages" />
        <EmptyState
          icon={PackageIcon}
          title="No package manager found"
          hint="Orbit works with winget, Chocolatey and Scoop. Install one and reopen this page."
        />
      </div>
    );
  }

  const summary = !manager
    ? 'Looking for package managers'
    : list.isFetching
      ? 'Reading packages (this can take a few seconds)'
      : list.data
        ? view === 'search'
          ? `${list.data.length} results`
          : `${list.data.length} ${view === 'installed' ? 'installed' : 'with updates'}`
        : ' ';

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Packages"
        summary={summary}
        refreshing={list.isFetching}
        actions={
          <>
            {view === 'outdated' && (list.data?.length ?? 0) > 0 && manager && (
              <Button
                variant="primary"
                disabled={busy}
                onClick={() =>
                  start({
                    kind: 'upgradeAll',
                    manager,
                    label: `Upgrade all with ${MANAGER_LABEL[manager]}`,
                  })
                }
              >
                Upgrade all
              </Button>
            )}
          </>
        }
      />
      <div className="flex flex-wrap items-center gap-3">
        {available.length > 1 && (
          <Segmented
            label="Package manager"
            value={manager ?? available[0]!}
            onChange={setPicked}
            options={available.map((m) => ({ value: m, label: MANAGER_LABEL[m] }))}
          />
        )}
        <Segmented
          label="List"
          value={view}
          onChange={setView}
          options={[
            { value: 'installed', label: 'Installed' },
            { value: 'outdated', label: 'Updates' },
            { value: 'search', label: 'Search' },
          ]}
        />
        {view === 'search' ? (
          <form
            className="flex items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              setQuery(draft.trim());
            }}
          >
            <SearchInput
              aria-label="Search packages"
              placeholder="Find a package to install"
              className="w-80"
              value={draft}
              maxLength={200}
              onChange={(e) => setDraft(e.target.value)}
            />
            <Button type="submit" disabled={!draft.trim() || draft.trim().startsWith('-')}>
              Search
            </Button>
          </form>
        ) : (
          <SearchInput
            aria-label="Filter packages"
            placeholder="Filter by name or id"
            className="w-80"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        )}
      </div>

      {run && <OutputDrawer run={run} onClose={close} />}

      {view === 'search' && !query ? (
        <EmptyState
          icon={Search}
          title="Search for a package"
          hint={`Type a name and press Enter to search ${manager ? MANAGER_LABEL[manager] : 'your package manager'}.`}
        />
      ) : list.isError ? (
        <EmptyState
          icon={PackageIcon}
          title="Could not read packages"
          hint={list.error.message}
          action={<Button onClick={refresh}>Try again</Button>}
        />
      ) : list.isPending ? (
        <div aria-busy className="h-40 animate-pulse rounded-card bg-raised" />
      ) : rows.length === 0 ? (
        <EmptyState
          icon={PackageIcon}
          title={
            view === 'outdated'
              ? 'Everything is up to date'
              : view === 'search'
                ? 'No packages found'
                : 'Nothing to show'
          }
          hint={filter ? 'Nothing matches the filter.' : undefined}
        />
      ) : (
        manager && (
          <PackageTable
            rows={rows}
            view={view}
            busy={busy}
            onInstall={(p) =>
              start({ kind: 'install', manager, id: p.id, label: `Install ${p.name}` })
            }
            onUpgrade={(p) =>
              start({ kind: 'upgrade', manager, id: p.id, label: `Upgrade ${p.name}` })
            }
            onUninstall={(p) =>
              start({ kind: 'uninstall', manager, id: p.id, label: `Uninstall ${p.name}` })
            }
          />
        )
      )}
    </div>
  );
}
