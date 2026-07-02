import type { ProgressEvent } from '../lib/types';

export function ProgressLog({ progress }: { progress: ProgressEvent[] }) {
  return (
    <section className="progress-log panel-scroll">
      <div className="panel-header">
        <h3>Backend progress</h3>
        <span>{progress.length}</span>
      </div>

      <div className="panel-scroll-body pretty-scroll">
        {progress.length === 0 && <p className="empty-state">No backend progress for this account yet.</p>}

        {progress.map((event, index) => (
          <article key={`${event.opId}-${event.at}-${index}`} className="progress-item">
            <div className="activity-title-row">
              <strong>{event.step}</strong>
              <time>{new Date(event.at).toLocaleTimeString()}</time>
            </div>

            <p>{event.message}</p>
          </article>
        ))}
      </div>
    </section>
  );
}
