import { act, render } from "@testing-library/react-native";
import { FirewallNotice } from "../src/ui/firewall-notice";

test("remote firewall help explains host authorization and exposes selectable repair commands", () => {
  const command = "sudo ufw allow from 192.168.5.0/24 to any port 47990 proto tcp";
  const view = render(
    <FirewallNotice
      warning={{
        subnet: "192.168.5.0/24",
        port: 47990,
        message: "UFW is blocking on-air audio from the local network.",
        repair_command: command,
      }}
    />,
  );
  expect(view.UNSAFE_queryAllByProps({ testID: "firewall-command" })).toHaveLength(0);
  // Plain faceplate text: no red box around the warning.
  const box = view.UNSAFE_getByProps({ testID: "firewall-warning" }).props.style;
  expect(box.backgroundColor).toBeUndefined();
  expect(box.borderWidth).toBeUndefined();
  act(() => view.UNSAFE_getByProps({ label: "Firewall help" }).props.onPress());
  const text = view.UNSAFE_getByProps({ testID: "firewall-command" });
  expect(text.props.selectable).toBe(true);
  expect(text.props.children).toBe(command);
  expect(JSON.stringify(view.toJSON())).toContain(
    "Administrator authentication happens on that computer",
  );
  act(() => view.UNSAFE_getByProps({ label: "Hide firewall help" }).props.onPress());
  expect(view.UNSAFE_queryAllByProps({ testID: "firewall-command" })).toHaveLength(0);
});
