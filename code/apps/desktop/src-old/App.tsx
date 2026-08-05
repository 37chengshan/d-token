import { useState, type ReactNode } from "react";
import {
  LayoutDashboard,
  Terminal,
  Gauge,
  Rocket,
  Bot,
  FolderGit2,
  Route as RouteIcon,
  Server,
  Settings2,
  Cpu,
  Stethoscope,
  ScrollText,
  GitCompareArrows,
  Archive,
  Sun,
  Moon,
  Zap,
} from "lucide-react";
import { useI18n, useTheme } from "./context";
import type { DictKey } from "./i18n";
import * as P from "./pages";

interface NavItem {
  id: string;
  icon: ReactNode;
  labelKey: DictKey;
  subKey: DictKey;
  badge?: string;
}

interface NavGroup {
  labelKey: DictKey;
  items: NavItem[];
}

const groups: NavGroup[] = [
  {
    labelKey: "navgroup.overview",
    items: [
      { id: "overview", icon: <LayoutDashboard size={18} />, labelKey: "nav.overview", subKey: "sub.overview" },
    ],
  },
  {
    labelKey: "navgroup.savings",
    items: [
      { id: "rtkDash", icon: <Terminal size={18} />, labelKey: "nav.rtkDash", subKey: "sub.rtkDash" },
      { id: "headroomDash", icon: <Gauge size={18} />, labelKey: "nav.headroomDash", subKey: "sub.headroomDash" },
    ],
  },
  {
    labelKey: "navgroup.config",
    items: [
      { id: "setup", icon: <Rocket size={18} />, labelKey: "nav.setup", subKey: "sub.setup" },
      { id: "agents", icon: <Bot size={18} />, labelKey: "nav.agents", subKey: "sub.agents" },
      { id: "projects", icon: <FolderGit2 size={18} />, labelKey: "nav.projects", subKey: "sub.projects" },
      { id: "routes", icon: <RouteIcon size={18} />, labelKey: "nav.routes", subKey: "sub.routes" },
      { id: "providers", icon: <Server size={18} />, labelKey: "nav.providers", subKey: "sub.providers" },
    ],
  },
  {
    labelKey: "navgroup.advanced",
    items: [
      { id: "rtk", icon: <Settings2 size={18} />, labelKey: "nav.rtk", subKey: "sub.rtk" },
      { id: "headroom", icon: <Cpu size={18} />, labelKey: "nav.headroom", subKey: "sub.headroom" },
    ],
  },
  {
    labelKey: "navgroup.audit",
    items: [
      { id: "doctor", icon: <Stethoscope size={18} />, labelKey: "nav.doctor", subKey: "sub.doctor", badge: "1" },
      { id: "logs", icon: <ScrollText size={18} />, labelKey: "nav.logs", subKey: "sub.logs" },
      { id: "changes", icon: <GitCompareArrows size={18} />, labelKey: "nav.changes", subKey: "sub.changes", badge: "2" },
      { id: "backup", icon: <Archive size={18} />, labelKey: "nav.backup", subKey: "sub.backup" },
    ],
  },
];

const allItems: NavItem[] = groups.flatMap((g) => g.items);
const settingsItem: NavItem = {
  id: "settings",
  icon: <Settings2 size={18} />,
  labelKey: "nav.settings",
  subKey: "sub.settings",
};

function renderPage(id: string, go: (id: string) => void) {
  switch (id) {
    case "overview":
      return <P.Overview go={go} />;
    case "rtkDash":
      return <P.RtkDashboard />;
    case "headroomDash":
      return <P.HeadroomDashboard />;
    case "setup":
      return <P.Setup />;
    case "agents":
      return <P.Agents />;
    case "projects":
      return <P.Projects />;
    case "routes":
      return <P.Routes />;
    case "providers":
      return <P.Providers />;
    case "rtk":
      return <P.RtkConfig />;
    case "headroom":
      return <P.HeadroomConfig />;
    case "doctor":
      return <P.Doctor />;
    case "logs":
      return <P.Logs />;
    case "changes":
      return <P.Changes />;
    case "backup":
      return <P.Backup />;
    case "settings":
      return <P.Settings />;
    default:
      return <P.Overview go={go} />;
  }
}

export default function App() {
  const { t, lang, setLang } = useI18n();
  const { theme, toggleTheme } = useTheme();
  const [active, setActive] = useState("overview");

  const current =
    allItems.find((i) => i.id === active) ??
    (active === "settings" ? settingsItem : allItems[0]);

  return (
    <div className="app">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-logo">
            <Zap size={20} />
          </div>
          <div>
            <div className="brand-name">{t("app.brand")}</div>
            <div className="brand-sub">{t("app.brandSub")}</div>
          </div>
        </div>

        <nav className="nav">
          {groups.map((g) => (
            <div key={g.labelKey}>
              <div className="nav-group-label">{t(g.labelKey)}</div>
              {g.items.map((it) => (
                <button
                  key={it.id}
                  className={`nav-item ${active === it.id ? "active" : ""}`}
                  onClick={() => setActive(it.id)}
                >
                  {it.icon}
                  <span>{t(it.labelKey)}</span>
                  {it.badge && <span className="nav-badge">{it.badge}</span>}
                </button>
              ))}
            </div>
          ))}
        </nav>

        <div className="sidebar-footer">
          <button
            className={`nav-item ${active === "settings" ? "active" : ""}`}
            onClick={() => setActive("settings")}
          >
            <Settings2 size={18} />
            <span>{t("nav.settings")}</span>
          </button>
        </div>
      </aside>

      {/* Main */}
      <div className="main">
        <header className="topbar">
          <div className="topbar-title">
            <h1>{t(current.labelKey)}</h1>
            <p>{t(current.subKey)}</p>
          </div>
          <div className="topbar-spacer" />
          <div className="topbar-actions">
            <div className="lang-toggle" title={t("set.language")}>
              <button
                className={lang === "zh" ? "active" : ""}
                onClick={() => setLang("zh")}
              >
                中
              </button>
              <button
                className={lang === "en" ? "active" : ""}
                onClick={() => setLang("en")}
              >
                EN
              </button>
            </div>
            <button
              className="icon-btn"
              onClick={toggleTheme}
              title={t("action.toggleTheme")}
              aria-label={t("action.toggleTheme")}
            >
              {theme === "dark" ? <Sun size={18} /> : <Moon size={18} />}
            </button>
          </div>
        </header>

        <main className="content">
          {/* key forces remount so page-in animation replays on nav change */}
          <div key={active}>{renderPage(active, setActive)}</div>
        </main>
      </div>
    </div>
  );
}
