import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useToastStore } from "./toastStore";

describe("toastStore", () => {
  beforeEach(() => {
    useToastStore.setState({ toasts: [] });
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("starts empty", () => {
    expect(useToastStore.getState().toasts).toHaveLength(0);
  });

  it("addToast defaults to type 'success'", () => {
    useToastStore.getState().addToast("Hello");
    const toasts = useToastStore.getState().toasts;
    expect(toasts).toHaveLength(1);
    expect(toasts[0]!.type).toBe("success");
    expect(toasts[0]!.message).toBe("Hello");
    expect(toasts[0]!.id).toBeTruthy();
  });

  it("respects an explicit type", () => {
    useToastStore.getState().addToast("Boom", "error");
    expect(useToastStore.getState().toasts[0]!.type).toBe("error");
  });

  it("each toast gets a unique id", () => {
    useToastStore.getState().addToast("a");
    useToastStore.getState().addToast("b");
    useToastStore.getState().addToast("c");
    const ids = useToastStore.getState().toasts.map((t) => t.id);
    expect(new Set(ids).size).toBe(3);
  });

  it("removeToast drops the matching id", () => {
    useToastStore.getState().addToast("keep");
    useToastStore.getState().addToast("drop");
    const dropId = useToastStore.getState().toasts[1]!.id;
    useToastStore.getState().removeToast(dropId);
    const remaining = useToastStore.getState().toasts;
    expect(remaining).toHaveLength(1);
    expect(remaining[0]!.message).toBe("keep");
  });

  it("removeToast is a no-op for unknown ids", () => {
    useToastStore.getState().addToast("only");
    useToastStore.getState().removeToast("nope");
    expect(useToastStore.getState().toasts).toHaveLength(1);
  });

  it("auto-removes a toast after 3500ms", () => {
    useToastStore.getState().addToast("temporary");
    expect(useToastStore.getState().toasts).toHaveLength(1);
    vi.advanceTimersByTime(3499);
    expect(useToastStore.getState().toasts).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(useToastStore.getState().toasts).toHaveLength(0);
  });

  it("manual removal cancels the auto-remove timer", () => {
    useToastStore.getState().addToast("manual");
    const id = useToastStore.getState().toasts[0]!.id;
    useToastStore.getState().removeToast(id);
    // Advance past the auto-remove window; should not throw or
    // double-remove.
    vi.advanceTimersByTime(5000);
    expect(useToastStore.getState().toasts).toHaveLength(0);
  });
});
