import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { api } from "@/api/client";
import type { ItemView } from "@/api/types/ItemView";
import type { PatchReply } from "@/api/types/PatchReply";
import type { PatchRequest } from "@/api/types/PatchRequest";
import { useConfig } from "@/api/useConfig";
import { columnLabel } from "@/components/board/model";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { itemUrl } from "@/components/item/useItem";
import { textField } from "@/components/planning/progress";
import { Button } from "@/components/ui/button";

const CONTROL = "rounded-[7px] border border-input bg-card px-[11px] py-[5px] text-[13px] text-foreground disabled:opacity-50";

/** One owner write: the change goes to the item's file with the etag this view was read at, then every list and page reads it again. */
function useOwnerWrite(item: { id: string; etag: string }, onDone: () => void) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (request: Omit<PatchRequest, "if_match">) => api.patch<PatchReply>(itemUrl(item.id), { ...request, if_match: item.etag }),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ["items"] });
      onDone();
    },
  });
}

/** The board's row size, for a decision made in a list. */
const SMALL = "h-auto px-2.5 py-[3px] text-[12.5px]";

/** Accept or send back a task that waits in review; nothing for a task in any other status. */
export function Decision({ task, small = false, onDone }: { task: { id: string; etag: string; status: string | null }; small?: boolean; onDone: () => void }) {
  const [sending, setSending] = useState(false);
  const [comment, setComment] = useState("");
  const field = useId();
  const write = useOwnerWrite(task, () => {
    setSending(false);
    setComment("");
    onDone();
  });
  if (task.status !== "review") return null;
  const blank = comment.trim() === "";
  return (
    <>
      <Button size="sm" className={small ? SMALL : undefined} disabled={write.isPending} onClick={() => write.mutate({ set: { status: "done" } })}>
        Accept
      </Button>
      <Button
        size="sm"
        variant="outline"
        className={small ? SMALL : undefined}
        disabled={write.isPending}
        onClick={() => {
          write.reset();
          setSending(true);
        }}
      >
        Send back
      </Button>
      {sending ? (
        <form
          className="flex basis-full flex-col items-start gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (!blank) write.mutate({ set: { status: "todo" }, note: `Sent back: ${comment}` });
          }}
        >
          <label htmlFor={field} className="text-xs text-muted-foreground">
            What has to change
          </label>
          <textarea id={field} value={comment} onChange={(event) => setComment(event.target.value)} rows={3} className={`${CONTROL} w-full max-w-xl`} />
          <div className="flex gap-2">
            <Button type="submit" size="sm" disabled={blank || write.isPending}>
              Send back
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => {
                write.reset();
                setSending(false);
              }}
            >
              Cancel
            </Button>
          </div>
        </form>
      ) : null}
      {write.isError ? (
        <div className="basis-full">
          <ErrorNotice error={write.error} />
        </div>
      ) : null}
    </>
  );
}

/** The task's status as a select of the configured statuses; choosing one writes it. */
export function StatusPicker({ item, onDone }: { item: ItemView; onDone: () => void }) {
  const config = useConfig();
  const write = useOwnerWrite(item, onDone);
  const current = textField(item.fields, "status") ?? "";
  const known = config.data?.statuses.task ?? [];
  const options = current === "" || known.includes(current) ? known : [current, ...known];
  return (
    <>
      <select aria-label="Status" value={current} disabled={write.isPending} onChange={(event) => write.mutate({ set: { status: event.target.value } })} className={CONTROL}>
        {options.map((status) => (
          <option key={status} value={status}>
            {columnLabel(status)}
          </option>
        ))}
      </select>
      {write.isError ? (
        <div className="mt-2">
          <ErrorNotice error={write.error} />
        </div>
      ) : null}
    </>
  );
}
