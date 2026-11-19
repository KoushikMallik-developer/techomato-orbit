import { describe, expect, it } from 'vitest';
import { defaultFormValues, formToSpec, specToForm } from './formToSpec';

const base = () => ({
  ...defaultFormValues(),
  name: 'API',
  executable: 'node',
  workingDir: 'C:/work/api',
});

describe('formToSpec', () => {
  it('splits arguments one per non-empty line and keeps quotes literally', () => {
    const spec = formToSpec({
      ...base(),
      argsText: 'server.js\r\n\r\n--name="a b"\n  \n--port 3000',
    });
    expect(spec.args).toEqual(['server.js', '--name="a b"', '--port 3000']);
  });

  it('turns blank numbers into null/defaults and parses set ones', () => {
    const spec = formToSpec({ ...base(), port: '', startupTimeoutSecs: '45' });
    expect(spec.port).toBeNull();
    expect(spec.startupTimeoutSecs).toBe(45);
    expect(formToSpec({ ...base(), port: '8080' }).port).toBe(8080);
  });

  it('rejects a secret variable without a key', () => {
    expect(() =>
      formToSpec({ ...base(), env: [{ key: ' ', value: 'x', isSecret: true }] }),
    ).toThrow(/key/i);
  });

  it('drops a blank non-secret row and sends an unchanged secret as null', () => {
    const spec = formToSpec({
      ...base(),
      env: [
        { key: '', value: '', isSecret: false },
        { key: 'TOKEN', value: '', isSecret: true },
        { key: 'MODE', value: 'dev', isSecret: false },
      ],
    });
    expect(spec.env).toEqual([
      { key: 'TOKEN', value: null, isSecret: true },
      { key: 'MODE', value: 'dev', isSecret: false },
    ]);
  });

  it('builds the restart policy and health check', () => {
    const spec = formToSpec({
      ...base(),
      restartKind: 'onFailure',
      maxRetries: '2',
      backoffSecs: '3',
      healthKind: 'http',
      healthUrl: 'http://localhost:8080/health',
      healthMin: '200',
      healthMax: '399',
    });
    expect(spec.restart).toEqual({ kind: 'onFailure', maxRetries: 2, backoffSecs: 3 });
    expect(spec.health).toMatchObject({ kind: 'http', minStatus: 200, maxStatus: 399 });
    expect(formToSpec(base()).health).toBeNull();
  });

  it('round-trips a spec through the form', () => {
    const spec = formToSpec({
      ...base(),
      argsText: 'a\nb',
      port: '9000',
      healthKind: 'tcp',
      healthPort: '9000',
    });
    expect(formToSpec(specToForm(spec))).toEqual(spec);
  });
});
