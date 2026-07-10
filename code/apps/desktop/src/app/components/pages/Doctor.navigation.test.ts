import { describe, expect, it } from "vitest";

import {
  DOCTOR_READINESS_TITLE,
  doctorFailureSummary,
  resolveDoctorDestination,
} from "./Doctor";

describe("resolveDoctorDestination", () => {
  it("maps legacy Doctor targets to pages that the current shell renders", () => {
    expect(resolveDoctorDestination("advanced")).toBe("install");
    expect(resolveDoctorDestination("logs")).toBe("changes");
    expect(resolveDoctorDestination("changes")).toBe("changes");
  });

  it("uses readable Chinese copy for diagnostic failures and readiness", () => {
    expect(doctorFailureSummary(new Error("native_unavailable"))).toBe(
      "当前是浏览器预览，请在 d-token 桌面窗口中运行完整诊断。",
    );
    expect(doctorFailureSummary(new Error("boom"))).toBe("诊断失败：boom");
    expect(DOCTOR_READINESS_TITLE).toBe("产品就绪检查");
  });
});
