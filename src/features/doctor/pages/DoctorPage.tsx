import { useMemo, useState } from 'react';
import {
  CircleCheck,
  Info,
  OctagonAlert,
  Stethoscope,
  TriangleAlert,
  type LucideIcon,
} from 'lucide-react';
import { Button } from '@/shared/ui/Button';
import { Card } from '@/shared/ui/Card';
import { EmptyState } from '@/shared/ui/EmptyState';
import { PageHeader } from '@/shared/ui/PageHeader';
import { Pill } from '@/shared/ui/Pill';
import type { Finding, Fix, Severity } from '../api/doctor.schema';
import { useDoctor } from '../api/useDoctor';
import { attentionCount, CATEGORY_TITLE, groupFindings } from '../doctorGroups';

const ICON: Record<Severity, { icon: LucideIcon; cls: string; label: string }> = {
  ok: { icon: CircleCheck, cls: 'text-running', label: 'OK' },
  info: { icon: Info, cls: 'text-fg-subtle', label: 'Info' },
  warn: { icon: TriangleAlert, cls: 'text-warn', label: 'Needs attention' },
  critical: { icon: OctagonAlert, cls: 'text-danger', label: 'Critical' },
};

const TAG_LABEL = { speed: 'Speed', storage: 'Storage', info: 'Info' } as const;

function fixLabel(fix: Fix): string {
  switch (fix.kind) {
    case 'clearCache':
      return 'Clear';
    case 'disableStartup':
      return 'Disable';
    case 'setPowerPlan':
      return 'Switch to Balanced';
  }
}

function Row({ f, busy, onFix }: { f: Finding; busy: boolean; onFix: (fix: Fix) => void }) {
  const { icon: Icon, cls, label } = ICON[f.severity];
  return (
    <li className="flex items-start gap-3 border-b border-border py-3 last:border-0">
      <Icon aria-label={label} role="img" className={`mt-0.5 size-5 shrink-0 ${cls}`} />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-medium">{f.title}</span>
          <Pill>{TAG_LABEL[f.tag]}</Pill>
        </div>
        <p className="mt-0.5 break-words text-sm text-fg-muted">{f.detail}</p>
      </div>
      {f.fix && (
        <Button size="sm" disabled={busy} onClick={() => onFix(f.fix!)}>
          {fixLabel(f.fix)}
        </Button>
      )}
    </li>
  );
}

const SHOWN = 5;

export function DoctorPage() {
  const { findings, scanning, millis, scan, fix } = useDoctor();
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const groups = useMemo(() => groupFindings(findings), [findings]);
  const attention = attentionCount(findings);

  const summary = scanning
    ? `Checking your PC… ${findings.length} checked so far`
    : findings.length === 0
      ? 'Nothing to report'
      : `${attention === 0 ? 'Nothing needs attention' : `${attention} need${attention === 1 ? 's' : ''} attention`}${
          millis !== null ? ` · scanned in ${(millis / 1000).toFixed(1)} s` : ''
        }`;

  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="Doctor" summary={summary} onRefresh={scan} refreshing={scanning} />
      <p role="status" className="sr-only">
        {scanning ? 'Checking your PC' : summary}
      </p>
      <p className="max-w-2xl text-sm text-fg-muted">
        Doctor only looks. Nothing changes unless you press a button and confirm. Tags say what a
        finding is about: <b>Speed</b> affects how fast the PC feels, <b>Storage</b> is disk space
        (clearing it does not make the PC faster), <b>Info</b> is just good to know.
      </p>
      {groups.length === 0 && !scanning && (
        <EmptyState icon={Stethoscope} title="No results" hint="Press Refresh to check." />
      )}
      {groups.map((g) => {
        const open = expanded[g.category] || g.items.length <= SHOWN + 1;
        const shown = open ? g.items : g.items.slice(0, SHOWN);
        return (
          <Card key={g.category}>
            <h2 className="text-lg font-bold">{CATEGORY_TITLE[g.category] ?? g.category}</h2>
            <ul>
              {shown.map((f) => (
                <Row key={f.id} f={f} busy={fix.isPending} onFix={(x) => fix.mutate(x)} />
              ))}
            </ul>
            {!open && (
              <Button
                size="sm"
                className="mt-2"
                onClick={() => setExpanded((e) => ({ ...e, [g.category]: true }))}
              >
                Show all {g.items.length}
              </Button>
            )}
          </Card>
        );
      })}
    </div>
  );
}
