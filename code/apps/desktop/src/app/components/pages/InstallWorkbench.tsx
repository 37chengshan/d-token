// 安装与连接工作台：4 个上栏标签（一键安装 / 智能学习 / 高级安装 / 日志）。
// 关联计划：release-plan-2026-07/phase-01 §3 安装与连接页内部信息架构、implementation-kickoff Sprint 1。
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Rocket, ScanSearch, ScrollText, SlidersHorizontal } from "lucide-react";
import { useT } from "../../lib/i18n";
import { useState } from "react";
import { Install } from "./Install";
import { SmartLearning } from "./SmartLearning";
import { Advanced } from "./Advanced";
import { LogsSecurity } from "./LogsSecurity";

export function InstallWorkbench() {
  const t = useT();
  const [activeTab, setActiveTab] = useState("one-click");
  const workbenchTabs = [
    {
      value: "one-click",
      title: t("in.tab.oneClick"),
      description: t("in.tab.oneClickDesc"),
      icon: Rocket,
    },
    {
      value: "smart-learning",
      title: t("in.tab.smartLearning"),
      description: t("in.tab.smartLearningDesc"),
      icon: ScanSearch,
    },
    {
      value: "advanced",
      title: t("in.tab.advanced"),
      description: t("in.tab.advancedDesc"),
      icon: SlidersHorizontal,
    },
    {
      value: "logs",
      title: t("in.tab.logs"),
      description: t("in.tab.logsDesc"),
      icon: ScrollText,
    },
  ] as const;

  return (
    <div>
      <Tabs value={activeTab} onValueChange={setActiveTab} className="gap-5">
        <TabsList className="install-workbench-nav grid h-20 w-full grid-cols-4 overflow-hidden rounded-lg border border-border/70 bg-card/80 p-0 shadow-sm">
          {workbenchTabs.map((item) => {
            const Icon = item.icon;
            return (
              <TabsTrigger
                key={item.value}
                value={item.value}
                className="install-workbench-nav-item relative h-full min-w-0 flex-none flex-col items-stretch justify-center gap-1 rounded-none border-0 border-r border-border/60 px-3 py-2 text-left whitespace-normal last:border-r-0"
              >
                <span className="flex min-w-0 items-center gap-2">
                  <span className="install-workbench-nav-icon flex size-7 shrink-0 items-center justify-center rounded-md">
                    <Icon aria-hidden="true" className="size-4" />
                  </span>
                  <span className="truncate text-sm font-medium text-foreground">{item.title}</span>
                </span>
                <span className="install-workbench-nav-description pl-9 text-xs font-normal text-muted-foreground">
                  {item.description}
                </span>
              </TabsTrigger>
            );
          })}
        </TabsList>
        <TabsContent value="one-click">
          <Install onOpenAdvanced={() => setActiveTab("advanced")} />
        </TabsContent>
        <TabsContent value="smart-learning">
          <SmartLearning />
        </TabsContent>
        <TabsContent value="advanced">
          <Advanced />
        </TabsContent>
        <TabsContent value="logs">
          <LogsSecurity />
        </TabsContent>
      </Tabs>
    </div>
  );
}
