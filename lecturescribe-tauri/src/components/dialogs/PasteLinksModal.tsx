import { Button, Field, Modal } from "../ui";

export function PasteLinksModal({
  open,
  value,
  onChange,
  onClose,
  onAdd,
}: {
  open: boolean;
  value: string;
  onChange: (value: string) => void;
  onClose: () => void;
  onAdd: () => void;
}) {
  const linkCount = countLinks(value);
  return (
    <Modal
      description="Paste one or many YouTube or Google Drive links. Duplicate items are kept out of the run automatically."
      footer={
        <>
          <span className="modal-footer-note">{linkCount} link{linkCount === 1 ? "" : "s"} detected</span>
          <Button onClick={onClose}>Cancel</Button>
          <Button disabled={linkCount === 0} icon="link" onClick={onAdd} variant="primary">Add links</Button>
        </>
      }
      onClose={onClose}
      open={open}
      title="Paste links"
    >
      <Field
        hint="Links may be separated by spaces, commas, or new lines. Playlist expansion is confirmed before large batches."
        label="Video or Drive links"
      >
        <textarea
          autoFocus
          className="links-textarea"
          onInput={(event) => onChange(event.currentTarget.value)}
          placeholder="https://www.youtube.com/watch?v=...&#10;https://drive.google.com/file/d/..."
          value={value}
        />
      </Field>
    </Modal>
  );
}

export function countLinks(value: string): number {
  return value.match(/https?:\/\/[^\s<>'"\])}]+/gi)?.length ?? 0;
}
