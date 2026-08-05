import { describe, expect, it } from "vitest";
import { presentOperationFailure } from "./userCopy";

describe("presentOperationFailure", () => {
  it("turns native preview failures into an actionable bilingual message", () => {
    expect(presentOperationFailure("connection", new Error("native_unavailable"), "zh")).toMatchObject({
      title: "当前为浏览器预览",
      message: "浏览器预览不会读取或修改本机配置。请在 d-token 桌面窗口中继续操作。",
      details: "状态代码：NATIVE_UNAVAILABLE",
    });
    expect(presentOperationFailure("connection", new Error("native_unavailable"), "en")).toMatchObject({
      title: "Browser preview",
      details: "Status code: NATIVE_UNAVAILABLE",
    });
  });

  it("does not surface raw native prose in an unknown failure", () => {
    const failure = presentOperationFailure(
      "route",
      new Error("Review this test fixture and retry with a secret value"),
      "en",
    );

    expect(failure.title).toBe("Connection path configuration did not finish");
    expect(failure.message).toBe("A credential could not be used safely. Confirm it in the system credential store, then try again.");
    expect(`${failure.title} ${failure.message} ${failure.details}`).not.toMatch(/review|fixture|secret/i);
  });

  it("explains an occupied port without exposing the native error", () => {
    const failure = presentOperationFailure("service", "bind failed: address already in use", "zh");
    expect(failure).toEqual({
      title: "本地服务操作未完成",
      message: "端口正在被其他程序使用。请关闭占用程序或调整端口后重试。",
      details: "状态代码：PORT_IN_USE",
    });
  });
});
