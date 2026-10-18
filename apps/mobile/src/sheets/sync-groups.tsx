import { Button, Row, Spacer, Text } from "@expo/ui";
import type { OutputInfo, OutputSyncStatus, SyncGroupView } from "@on-air/api-types";
import { SYNC_GROUP_DESCRIPTION } from "@on-air/control-client";
import { useState } from "react";
import { TextInput, View } from "react-native";
import { colors, radius, theme } from "@/theme";

/** One speaker as the groups area sees it: its catalog entry and its sync row. */
export type SyncSpeaker = { output: OutputInfo; status: OutputSyncStatus };

/** The chooser spells out both consequences before a membership changes. */
const CHOOSER_COPY =
  "Adding changes nothing until the group starts together. Leaving stops this speaker if it is playing along.";

/** One speaker's membership line: which group it plays with, if any. */
export function membershipSummary(row: OutputSyncStatus, groups: SyncGroupView[]): string {
  const group = groups.find((candidate) => candidate.id === row.group_id);
  return group ? `Playing with ${group.name}` : "Playing alone";
}

/** One group's card body: its speakers and whether they play together now. */
export function groupSummary(group: SyncGroupView): string {
  const speakers = group.members.map((member) => member.device_name).join(", ");
  return `${speakers || "No speakers yet"} · ${group.playing ? "playing together now" : "not playing"}`;
}

/**
 * The sheet's playing-together area: SEVERAL SPEAKERS playing the same
 * audio in sync, never one merged device. Every latency figure stays an
 * uncalibrated default and each speaker keeps its own volume and trim.
 */
export function SyncGroupsPanel({
  speakers,
  groups,
  disabled,
  onCreate,
  onJoin,
  onLeave,
  onStart,
  onStop,
  onDissolve,
}: {
  speakers: SyncSpeaker[];
  groups: SyncGroupView[];
  disabled: boolean;
  onCreate(name: string): Promise<boolean>;
  onJoin(output: OutputInfo, groupId: number): Promise<boolean>;
  onLeave(output: OutputInfo): Promise<boolean>;
  onStart(groupId: number): Promise<void>;
  onStop(groupId: number): Promise<void>;
  onDissolve(groupId: number): Promise<void>;
}) {
  const [draft, setDraft] = useState("");
  const [openFor, setOpenFor] = useState<string | null>(null);
  const [working, setWorking] = useState(false);
  const name = draft.trim();
  const canCreate = name.length >= 1 && name.length <= 64;
  const off = disabled || working;

  /** Runs one group action with the panel locked until it settles. */
  const run = async (action: () => Promise<void>) => {
    setWorking(true);
    try {
      await action();
    } finally {
      setWorking(false);
    }
  };

  /** A move is two sequential calls on the wire: out of the old, into the new. */
  const pick = async (speaker: SyncSpeaker, groupId: number | null) => {
    setOpenFor(null);
    const current = speaker.status.group_id ?? null;
    if (current === groupId) return;
    setWorking(true);
    try {
      if (current !== null && !(await onLeave(speaker.output))) return;
      if (groupId !== null) await onJoin(speaker.output, groupId);
    } finally {
      setWorking(false);
    }
  };

  return (
    <View
      style={{
        borderWidth: 1,
        borderColor: colors.border,
        borderRadius: theme.radius.md,
        padding: theme.spacing.md,
        gap: theme.spacing.sm,
      }}
      testID="sync-groups"
    >
      <Text textStyle={{ fontWeight: "600" }}>Playing together</Text>
      <Text textStyle={{ color: colors.secondaryLabel }}>{SYNC_GROUP_DESCRIPTION}</Text>
      {speakers.map((speaker) => {
        const { output, status } = speaker;
        const key = `${output.transport}-${output.id}`;
        const open = openFor === key;
        return (
          <View key={key} style={{ gap: theme.spacing.xs }}>
            <Button
              label={`${output.name} · ${membershipSummary(status, groups)}`}
              variant="outlined"
              disabled={off}
              onPress={() => setOpenFor(open ? null : key)}
              testID={`sync-membership-${key}`}
            />
            {open && (
              <View style={{ gap: theme.spacing.xs }} testID={`sync-chooser-${key}`}>
                <Text textStyle={{ color: colors.tertiaryLabel, fontSize: 13 }}>
                  {CHOOSER_COPY}
                </Text>
                <Button
                  label="Playing alone"
                  variant={status.group_id === undefined ? "filled" : "outlined"}
                  disabled={off}
                  onPress={() => void pick(speaker, null)}
                  testID={`sync-alone-${key}`}
                />
                {groups.map((group) => (
                  <Button
                    key={group.id}
                    label={group.name}
                    variant={status.group_id === group.id ? "filled" : "outlined"}
                    disabled={off}
                    onPress={() => void pick(speaker, group.id)}
                    testID={`sync-join-${group.id}-${key}`}
                  />
                ))}
              </View>
            )}
          </View>
        );
      })}
      {groups.map((group) => (
        <View key={group.id} style={{ gap: theme.spacing.sm }} testID={`sync-group-${group.id}`}>
          <Text textStyle={{ fontWeight: "600" }}>{group.name}</Text>
          <Text textStyle={{ color: colors.secondaryLabel }}>{groupSummary(group)}</Text>
          <Row alignment="center" spacing={theme.spacing.sm}>
            <Button
              label="Start together"
              variant="outlined"
              disabled={off || group.playing || group.members.length === 0}
              onPress={() => void run(() => onStart(group.id))}
              testID={`sync-group-start-${group.id}`}
            />
            <Button
              label="Stop"
              variant="outlined"
              disabled={off || !group.playing}
              onPress={() => void run(() => onStop(group.id))}
              testID={`sync-group-stop-${group.id}`}
            />
            <Spacer />
            <Button
              label="Dissolve"
              variant="text"
              disabled={off}
              onPress={() => void run(() => onDissolve(group.id))}
              testID={`sync-group-dissolve-${group.id}`}
            />
          </Row>
        </View>
      ))}
      <Text textStyle={{ fontWeight: "600" }}>New group</Text>
      <TextInput
        value={draft}
        onChangeText={setDraft}
        placeholder="Group name"
        editable={!off}
        testID="sync-group-name-input"
        style={{
          padding: 12,
          borderWidth: 1,
          borderColor: colors.border,
          borderRadius: radius.sm,
          color: colors.label,
        }}
      />
      {draft.length > 0 && !canCreate && (
        <Text textStyle={{ color: colors.secondaryLabel }}>Use 1 to 64 characters.</Text>
      )}
      <Button
        label="Create group"
        variant="outlined"
        disabled={off || !canCreate}
        onPress={() =>
          void run(async () => {
            if (await onCreate(name)) setDraft("");
          })
        }
        testID="sync-group-create"
      />
    </View>
  );
}
