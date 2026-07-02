export function ErrorModal({
  message,
  onClose,
}: {
  message: string;
  onClose: () => void;
}) {
  return (
    <div className="error-modal-backdrop" role="dialog" aria-modal="true">
      <div className="error-modal-card">
        <button className="error-modal-close" onClick={onClose} aria-label="Close error">
          ×
        </button>

        <div className="error-modal-kicker">Operation failed</div>
        <h3>Error</h3>
        <p>{message}</p>

        <button className="error-modal-ok" onClick={onClose}>
          Close
        </button>
      </div>
    </div>
  );
}
