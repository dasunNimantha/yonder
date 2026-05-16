import { beforeEach, describe, expect, it } from "vitest";

import type { ProgressEvent, Transfer, TransferStatus } from "../lib/tauri";
import { isActive, useTransferStore } from "./transferStore";

function transfer(id: string, status: TransferStatus = "active"): Transfer {
  return {
    id,
    direction: "send",
    peer_id: "peer-1",
    peer_name: "Bob",
    files: [{ name: "a.txt", size: 100 }],
    total_bytes: 100,
    bytes_done: 0,
    status,
    started_at: new Date().toISOString(),
  };
}

describe("transferStore", () => {
  beforeEach(() => {
    useTransferStore.setState({ transfers: [], pendingApproval: null });
  });

  it("upsertTransfer appends new and replaces existing in place", () => {
    useTransferStore.getState().upsertTransfer(transfer("a"));
    useTransferStore.getState().upsertTransfer(transfer("b"));
    expect(useTransferStore.getState().transfers.map((t) => t.id)).toEqual(["a", "b"]);

    const replaced = { ...transfer("a"), peer_name: "Charlie" };
    useTransferStore.getState().upsertTransfer(replaced);
    const list = useTransferStore.getState().transfers;
    expect(list[0]!.peer_name).toBe("Charlie");
    expect(list).toHaveLength(2);
  });

  it("applyProgress mutates the matching transfer's bytes_done and status", () => {
    useTransferStore.getState().upsertTransfer(transfer("a"));
    const evt: ProgressEvent = {
      id: "a",
      bytes_done: 42,
      total_bytes: 100,
      status: "active",
    };
    useTransferStore.getState().applyProgress(evt);
    const updated = useTransferStore.getState().transfers[0]!;
    expect(updated.bytes_done).toBe(42);
    expect(updated.status).toBe("active");
  });

  it("applyProgress preserves total_bytes if the event reports zero", () => {
    useTransferStore.getState().upsertTransfer(transfer("a"));
    useTransferStore.getState().applyProgress({
      id: "a",
      bytes_done: 50,
      total_bytes: 0,
      status: "active",
    });
    expect(useTransferStore.getState().transfers[0]!.total_bytes).toBe(100);
  });

  it("applyProgress is a no-op for unknown ids", () => {
    useTransferStore.getState().upsertTransfer(transfer("a"));
    useTransferStore.getState().applyProgress({
      id: "missing",
      bytes_done: 99,
      total_bytes: 100,
      status: "completed",
    });
    expect(useTransferStore.getState().transfers[0]!.bytes_done).toBe(0);
  });

  it("setPendingApproval gets / clears the modal state", () => {
    useTransferStore.getState().setPendingApproval(transfer("p"));
    expect(useTransferStore.getState().pendingApproval?.id).toBe("p");
    useTransferStore.getState().setPendingApproval(null);
    expect(useTransferStore.getState().pendingApproval).toBeNull();
  });

  it("clearCompleted keeps only non-terminal transfers", () => {
    useTransferStore.getState().setTransfers([
      transfer("a", "active"),
      transfer("b", "completed"),
      transfer("c", "failed"),
      transfer("d", "rejected"),
      transfer("e", "cancelled"),
      transfer("f", "awaiting-approval"),
      transfer("g", "pending"),
    ]);
    useTransferStore.getState().clearCompleted();
    const ids = useTransferStore.getState().transfers.map((t) => t.id);
    expect(ids).toEqual(["a", "f", "g"]);
  });
});

describe("isActive", () => {
  it("is true for in-flight states", () => {
    expect(isActive(transfer("x", "active"))).toBe(true);
    expect(isActive(transfer("x", "pending"))).toBe(true);
    expect(isActive(transfer("x", "awaiting-approval"))).toBe(true);
  });

  it("is false for terminal states", () => {
    expect(isActive(transfer("x", "completed"))).toBe(false);
    expect(isActive(transfer("x", "failed"))).toBe(false);
    expect(isActive(transfer("x", "cancelled"))).toBe(false);
    expect(isActive(transfer("x", "rejected"))).toBe(false);
  });
});
