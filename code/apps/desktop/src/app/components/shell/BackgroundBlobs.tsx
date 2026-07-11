// Three softly separated depth planes create a restrained stellar mist.
// Motion lives in CSS so prefers-reduced-motion can make the scene fully static.
export function BackgroundBlobs() {
  return (
    <div className="background-atmosphere" aria-hidden="true">
      <div className="background-stars" />
      <div className="background-nebula background-nebula--far" />
      <div className="background-nebula background-nebula--mid" />
      <div className="background-nebula background-nebula--near" />
    </div>
  );
}
