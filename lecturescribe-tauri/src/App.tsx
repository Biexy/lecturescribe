import { ActivityBar } from "./components/ActivityBar";
import { Header } from "./components/Header";
import { QueueWorkspace } from "./components/QueueWorkspace";
import { SourcePanel } from "./components/SourcePanel";
import { AboutModal, ItemDrawer, SummaryModal } from "./components/dialogs/ProductDialogs";
import { PasteLinksModal } from "./components/dialogs/PasteLinksModal";
import { PreflightModal } from "./components/dialogs/PreflightModal";
import { SettingsModal } from "./components/dialogs/SettingsModal";
import { SetupWizard } from "./components/dialogs/SetupWizard";
import { ToastRegion } from "./components/ui";
import { useAppController } from "./hooks/useAppController";
import { itemSnapshots } from "./state/app-state";

export function App() {
  const controller = useAppController();
  const {
    state,
    dispatch,
    pastedText,
    setPastedText,
    starting,
    settingsSaving,
    setupBusy,
    setupTest,
    actions,
  } = controller;

  if (state.booting) {
    return (
      <div className="loading-screen">
        <div className="brand-mark"><span className="spinner large" /></div>
        <h1>LectureScribe</h1>
        <p>Opening the local run ledger and checking setup...</p>
      </div>
    );
  }

  const detailItem = state.preview?.items.find((item) => item.id === state.detailItemId) ?? null;
  const detailSnapshot = detailItem ? itemSnapshots(state.job).get(detailItem.id) : undefined;

  return (
    <div className="app-shell">
      <Header
        environment={state.environment}
        onAbout={() => dispatch({ type: "dialog", dialog: "about" })}
        onOpenOutput={() => void actions.openOutput()}
        onSettings={() => dispatch({ type: "dialog", dialog: "settings" })}
        onSetup={() => dispatch({ type: "dialog", dialog: "setup" })}
        onTheme={actions.setTheme}
        settings={state.settings}
      />

      <div className="app-workspace">
        <SourcePanel
          onAddFolder={() => void actions.addFolder()}
          onAddMedia={() => void actions.addMedia()}
          onAddText={() => void actions.addTextFiles()}
          onClear={actions.clearSources}
          onMode={actions.setMode}
          onPaste={() => dispatch({ type: "dialog", dialog: "sources" })}
          onRefresh={actions.refreshPreview}
          onRemove={actions.removeSource}
          onReview={() => void actions.reviewPlan()}
          state={state}
        />
        <QueueWorkspace
          onDetail={(id) => dispatch({ type: "detail", id })}
          onFilter={actions.setFilter}
          onOpenArtifact={(itemId, kind, reveal) => void actions.openArtifact(itemId, kind, reveal)}
          onOpenHistory={(entry) => void actions.openHistory(entry)}
          onRefresh={actions.refreshPreview}
          onSearch={actions.setSearch}
          onSelect={actions.selectItems}
          onToggle={actions.toggleItem}
          onWorkspace={actions.setWorkspace}
          state={state}
        />
      </div>

      <ActivityBar
        onCancel={() => void actions.cancelJob()}
        onExpand={(expanded) => dispatch({ type: "activity", expanded })}
        onOpenOutput={() => void actions.openOutput()}
        onPause={() => void actions.pauseJob()}
        onResume={() => void actions.resumeJob()}
        onRetry={() => void actions.retryFailed()}
        state={state}
      />

      <PasteLinksModal
        onAdd={actions.addPastedLinks}
        onChange={setPastedText}
        onClose={() => dispatch({ type: "dialog", dialog: null })}
        open={state.dialog === "sources"}
        value={pastedText}
      />

      <PreflightModal
        onClose={() => dispatch({ type: "dialog", dialog: null })}
        onFixSetup={() => {
          dispatch({ type: "setup_step", step: setupStepForPlan(state.plan?.blocking_errors[0]?.code) });
          dispatch({ type: "dialog", dialog: "setup" });
        }}
        onStart={() => void actions.startPlan()}
        open={state.dialog === "preflight"}
        plan={state.plan}
        starting={starting}
      />

      <SetupWizard
        busy={setupBusy}
        environment={state.environment}
        onCheck={() => void actions.refreshEnvironment()}
        onChooseFfmpeg={() => void actions.chooseFfmpeg()}
        onChooseOutput={() => void actions.chooseOutput()}
        onClose={() => dispatch({ type: "dialog", dialog: null })}
        onDeleteKey={() => void actions.deleteApiKey()}
        onInstallDownloader={() => void actions.installDownloader()}
        onOpenLink={(target) => void actions.openLink(target)}
        onSaveKey={(key) => void actions.saveApiKey(key)}
        onStep={(step) => dispatch({ type: "setup_step", step })}
        onTest={() => void actions.runSetupTest()}
        open={state.dialog === "setup"}
        settings={state.settings}
        step={state.setupStep}
        testResult={setupTest}
      />

      <SettingsModal
        environment={state.environment}
        onChooseOutput={() => void actions.chooseOutput()}
        onClose={() => dispatch({ type: "dialog", dialog: null })}
        onExportDiagnostics={() => void actions.exportDiagnostics()}
        onOpenDiagnostics={() => {
          dispatch({ type: "setup_step", step: 5 });
          dispatch({ type: "dialog", dialog: "setup" });
        }}
        onSave={(settings) => void actions.saveSettings(settings)}
        open={state.dialog === "settings"}
        saving={settingsSaving}
        settings={state.settings}
      />

      <AboutModal
        environment={state.environment}
        onClose={() => dispatch({ type: "dialog", dialog: null })}
        onGitHub={() => void actions.openLink("github")}
        open={state.dialog === "about"}
      />

      <SummaryModal
        job={state.job}
        onClose={() => dispatch({ type: "dialog", dialog: null })}
        onOpenOutput={() => void actions.openOutput()}
        onRetry={() => void actions.retryFailed()}
        open={state.dialog === "summary"}
      />

      <ItemDrawer
        item={detailItem}
        onClose={() => dispatch({ type: "detail", id: null })}
        onOpenArtifact={(itemId, kind, reveal) => void actions.openArtifact(itemId, kind, reveal)}
        open={Boolean(detailItem)}
        settings={state.settings}
        snapshot={detailSnapshot}
      />

      <ToastRegion
        onAction={actions.handleToastAction}
        onDismiss={(id) => dispatch({ type: "dismiss_toast", id })}
        toasts={state.toasts}
      />
    </div>
  );
}

function setupStepForPlan(code?: string): number {
  if (code?.includes("api_key")) return 1;
  if (code?.includes("ffmpeg")) return 2;
  if (code?.includes("downloader")) return 3;
  if (code?.includes("output")) return 4;
  return 5;
}
