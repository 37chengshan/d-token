/**
 * d-token Showcase - Interactive Controller (Vanilla JS, Zero Dependencies)
 */

(function () {
  const PAGE_TITLES = {
    overview: "总体概览",
    install: "安装与连接",
    "codebase-memory": "代码图谱（实验性）",
    analytics: "分析",
    visual: "视觉压缩",
    doctor: "Doctor",
    changes: "变更与回退",
    settings: "系统设置",
  };

  let currentPage = "overview";

  function setPage(pageId) {
    if (!PAGE_TITLES[pageId]) return;
    currentPage = pageId;

    // 1. 切换页面内容
    document.querySelectorAll(".page-view").forEach((el) => {
      el.classList.toggle("active", el.id === `page-${pageId}`);
    });

    // 2. 更新侧边栏选中状态
    document.querySelectorAll(".shell-sidebar [data-nav-id]").forEach((btn) => {
      const active = btn.getAttribute("data-nav-id") === pageId;
      btn.classList.toggle("active", active);
      if (active) {
        btn.setAttribute("aria-current", "page");
      } else {
        btn.removeAttribute("aria-current");
      }
    });

    // 3. 更新顶部调试栏状态
    document.querySelectorAll(".showcase-dev-bar .nav-pill").forEach((btn) => {
      btn.classList.toggle("active", btn.getAttribute("data-page-target") === pageId);
    });

    // 4. 更新 AppShell Topbar 标题与工作台属性
    const titleEl = document.getElementById("topbar-page-title");
    if (titleEl) titleEl.textContent = PAGE_TITLES[pageId];

    const mainEl = document.querySelector("main.shell-content-scroll");
    if (mainEl) {
      if (pageId === "overview") {
        mainEl.removeAttribute("data-workbench");
      } else {
        mainEl.setAttribute("data-workbench", pageId);
      }
    }
  }

  function toggleSidebar() {
    const shell = document.querySelector(".app-shell");
    if (shell) {
      shell.classList.toggle("is-sidebar-collapsed");
      const isCollapsed = shell.classList.contains("is-sidebar-collapsed");
      const btn = document.getElementById("dev-toggle-sidebar");
      if (btn) btn.textContent = isCollapsed ? "展开侧栏" : "收起侧栏";
    }
  }

  function toggleTheme() {
    const isDark = document.documentElement.classList.toggle("dark");
    const btn = document.getElementById("dev-toggle-theme");
    if (btn) btn.textContent = isDark ? "☀️ 浅色" : "🌙 深色";
  }

  function openModal(modalId) {
    const modal = document.getElementById(modalId);
    if (modal) modal.classList.add("active");
  }

  function closeModal(modalId) {
    const modal = document.getElementById(modalId);
    if (modal) modal.classList.remove("active");
  }

  // 初始化交互绑定
  document.addEventListener("DOMContentLoaded", () => {
    // 默认开启深色模式
    document.documentElement.classList.add("dark");

    // 侧栏点击
    document.querySelectorAll(".shell-sidebar [data-nav-id]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = btn.getAttribute("data-nav-id");
        if (id) setPage(id);
      });
    });

    // 顶部调试栏胶囊点击
    document.querySelectorAll(".showcase-dev-bar .nav-pill").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = btn.getAttribute("data-page-target");
        if (id) setPage(id);
      });
    });

    // 折叠按钮
    const sidebarToggleBtn = document.getElementById("sidebar-toggle-trigger");
    if (sidebarToggleBtn) sidebarToggleBtn.addEventListener("click", toggleSidebar);

    const devSidebarBtn = document.getElementById("dev-toggle-sidebar");
    if (devSidebarBtn) devSidebarBtn.addEventListener("click", toggleSidebar);

    // 主题切换
    const themeBtn = document.getElementById("dev-toggle-theme");
    if (themeBtn) themeBtn.addEventListener("click", toggleTheme);

    // 弹窗关闭背景与取消按钮
    document.querySelectorAll(".modal-backdrop").forEach((backdrop) => {
      backdrop.addEventListener("click", (e) => {
        if (e.target === backdrop) {
          backdrop.classList.remove("active");
        }
      });
    });

    document.querySelectorAll("[data-close-modal]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const targetId = btn.getAttribute("data-close-modal");
        if (targetId) closeModal(targetId);
      });
    });

    // 弹窗打开触发器
    document.querySelectorAll("[data-open-modal]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const targetId = btn.getAttribute("data-open-modal");
        if (targetId) openModal(targetId);
      });
    });

    // 页面内子标签页切换 (如 InstallWorkbench 的一键安装/智能学习/高级/日志)
    document.querySelectorAll("[data-tab-group]").forEach((group) => {
      const groupId = group.getAttribute("data-tab-group");
      const triggers = group.querySelectorAll("[data-tab-target]");
      triggers.forEach((trigger) => {
        trigger.addEventListener("click", () => {
          const target = trigger.getAttribute("data-tab-target");
          triggers.forEach((t) => t.classList.toggle("active", t === trigger));
          document.querySelectorAll(`[data-tab-content-group="${groupId}"] .tab-pane`).forEach((pane) => {
            pane.classList.toggle("active", pane.getAttribute("data-tab-pane") === target);
          });
        });
      });
    });

    // 模拟操作通知触发
    document.querySelectorAll("[data-demo-toast]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const banner = document.getElementById("showcase-notification-banner");
        if (banner) {
          banner.style.display = "block";
          setTimeout(() => {
            banner.style.display = "none";
          }, 3500);
        }
      });
    });

    // 默认展示 Overview
    setPage("overview");
  });

  // 暴露全局便捷调用
  window.dtokenShowcase = {
    setPage,
    toggleSidebar,
    toggleTheme,
    openModal,
    closeModal,
  };
})();
