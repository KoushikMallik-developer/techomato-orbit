import type { ServiceFormValues } from './formToSpec';
import { defaultFormValues } from './formToSpec';

/** Starting points for "Add service". They only fill the form; nothing is installed or run. */
export type ServiceTemplate = {
  id: string;
  label: string;
  values: Partial<ServiceFormValues>;
};

export const SERVICE_TEMPLATES: ServiceTemplate[] = [
  {
    id: 'django',
    label: 'Django server',
    values: {
      name: 'Django server',
      category: 'backend',
      executable: 'python',
      argsText: 'manage.py\nrunserver\n8000',
      port: '8000',
    },
  },
  {
    id: 'flask',
    label: 'Flask server',
    values: {
      name: 'Flask server',
      category: 'backend',
      executable: 'flask',
      argsText: '--app\napp\nrun\n--port\n5000',
      port: '5000',
    },
  },
  {
    id: 'fastapi',
    label: 'FastAPI server',
    values: {
      name: 'FastAPI server',
      category: 'backend',
      executable: 'uvicorn',
      argsText: 'main:app\n--reload\n--port\n8000',
      port: '8000',
    },
  },
  {
    id: 'react-vite',
    label: 'React (Vite)',
    values: {
      name: 'React (Vite)',
      category: 'frontend',
      executable: 'npm',
      argsText: 'run\ndev',
      runThroughShell: true,
      port: '5173',
    },
  },
  {
    id: 'express',
    label: 'Express.js server',
    values: {
      name: 'Express.js server',
      category: 'backend',
      executable: 'node',
      argsText: 'server.js',
      port: '3000',
    },
  },
  {
    id: 'node',
    label: 'Node.js (npm run dev)',
    values: {
      name: 'Node.js app',
      category: 'backend',
      executable: 'npm',
      argsText: 'run\ndev',
      runThroughShell: true,
    },
  },
  {
    id: 'celery-worker',
    label: 'Celery worker',
    values: {
      name: 'Celery worker',
      category: 'worker',
      executable: 'celery',
      argsText: '-A\nmyapp\nworker\n--loglevel=info\n--pool=solo',
      restartKind: 'onFailure',
    },
  },
  {
    id: 'celery-beat',
    label: 'Celery beat',
    values: {
      name: 'Celery beat',
      category: 'worker',
      executable: 'celery',
      argsText: '-A\nmyapp\nbeat\n--loglevel=info',
      restartKind: 'onFailure',
    },
  },
  {
    id: 'docker-container',
    label: 'Docker container',
    values: {
      name: 'Docker container',
      category: 'custom',
      executable: 'docker',
      argsText: 'run\n--rm\n--name\nmy-container\n-p\n8080:80\nnginx',
      shutdownTimeoutSecs: '20',
    },
  },
  {
    id: 'docker-compose',
    label: 'Docker Compose',
    values: {
      name: 'Docker Compose',
      category: 'custom',
      executable: 'docker',
      argsText: 'compose\nup',
      shutdownTimeoutSecs: '30',
    },
  },
];

/** A fresh form from the template. Only the project, the folder and a name of the user's own are
 * kept; every other field is replaced, so pick a template first. */
export function applyTemplate(prev: ServiceFormValues, t: ServiceTemplate): ServiceFormValues {
  const typedName =
    prev.name.trim() !== '' && !SERVICE_TEMPLATES.some((x) => x.values.name === prev.name);
  return {
    ...defaultFormValues(),
    ...t.values,
    name: typedName ? prev.name : (t.values.name ?? ''),
    projectId: prev.projectId,
    workingDir: prev.workingDir,
  };
}
