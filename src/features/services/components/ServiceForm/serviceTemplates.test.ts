import { describe, expect, it } from 'vitest';
import { CATEGORIES } from '../../api/services.schema';
import { defaultFormValues, formToSpec } from './formToSpec';
import { applyTemplate, SERVICE_TEMPLATES } from './serviceTemplates';

describe('service templates', () => {
  it('have unique ids', () => {
    const ids = SERVICE_TEMPLATES.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('keeps what the user typed and chose, and drops the previous template', () => {
    const django = SERVICE_TEMPLATES.find((t) => t.id === 'django')!;
    const celery = SERVICE_TEMPLATES.find((t) => t.id === 'celery-worker')!;
    const typed = {
      ...defaultFormValues(),
      name: 'Shop API',
      projectId: 'p1',
      workingDir: 'C:\\shop',
    };
    const first = applyTemplate(typed, django);
    expect(first).toMatchObject({ name: 'Shop API', projectId: 'p1', workingDir: 'C:\\shop' });
    expect(first.port).toBe('8000');
    // Switching template: the template's own name changes, and its port does not linger.
    const second = applyTemplate(applyTemplate(defaultFormValues(), django), celery);
    expect(second.name).toBe('Celery worker');
    expect(second.port).toBe('');
  });

  it.each(SERVICE_TEMPLATES)('$label fills a form that converts to a valid spec', (t) => {
    const f = { ...defaultFormValues(), ...t.values };
    expect(CATEGORIES).toContain(f.category);
    const spec = formToSpec(f);
    expect(spec.name).not.toBe('');
    expect(spec.executable).not.toBe('');
    expect(spec.args.length).toBeGreaterThan(0);
  });
});
