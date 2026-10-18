import { useTranslation } from "react-i18next";
import type { AppConfig, Settings } from "../../lib/config";
import { PillTrack } from "../PillTrack";
import { HelpTip, SettingRow } from "../shared";

export interface AdvancedSettingsProps {
  activeConfig: AppConfig;
  updateSettings: (patch: Partial<Settings>) => void;
}

/** Advanced tab: capture tuning (CONSUMED / REPLAYED windows). The raw
 *  technical terms live in the "?" hint, not the label. */
export function AdvancedSettings({ activeConfig, updateSettings }: AdvancedSettingsProps) {
  const { t } = useTranslation();
  const osd = activeConfig.settings;

  return (
    <section className="settings-section">
      <div className="settings-section__header">
        <span className="settings-section__title">{t("settings.captureHeader")}</span>
      </div>

      <SettingRow
        title={
          <>
            {t("settings.modifierStaleGcLabel")}
            <HelpTip text={t("settings.modifierStaleGcHelp")} />
          </>
        }
        hint={t("settings.modifierStaleGcCaption")}
      >
        <PillTrack
          items={[1000, 3000, 5000, 10000].map((ms) => ({
            key: String(ms),
            label: t("settings.modifierStaleGcOption", { seconds: ms / 1000 }),
          }))}
          active={String(osd.modifierStaleGcMs ?? 5000)}
          onSelect={(k) =>
            updateSettings({ modifierStaleGcMs: Number(k) === 5000 ? undefined : Number(k) })
          }
        />
      </SettingRow>

      <SettingRow
        title={
          <>
            {t("settings.replayedForceReleaseLabel")}
            <HelpTip text={t("settings.replayedForceReleaseHelp")} />
          </>
        }
        hint={t("settings.replayedForceReleaseCaption")}
      >
        <PillTrack
          items={[5000, 15000, 30000, 60000].map((ms) => ({
            key: String(ms),
            label: t("settings.replayedForceReleaseOption", { seconds: ms / 1000 }),
          }))}
          active={String(osd.replayedModifierForceReleaseMs ?? 30000)}
          onSelect={(k) =>
            updateSettings({
              replayedModifierForceReleaseMs: Number(k) === 30000 ? undefined : Number(k),
            })
          }
        />
      </SettingRow>
    </section>
  );
}
