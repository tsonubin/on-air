import { formatDb, formatRate } from "@/readouts";

test("sample rate reads in kHz with at most one decimal", () => {
  expect(formatRate(44100)).toEqual({ text: "44.1K", spoken: "44.1 kilohertz stereo" });
  expect(formatRate(48000)).toEqual({ text: "48K", spoken: "48 kilohertz stereo" });
  expect(formatRate(88200).text).toBe("88.2K");
  expect(formatRate(96000).text).toBe("96K");
  expect(formatRate(0)).toEqual({ text: "--.-K", spoken: "sample rate unknown" });
});

test("decibels read signed on the LED and in words aloud", () => {
  expect(formatDb(3.5)).toEqual({ text: "+3.5", spoken: "plus 3.5 decibels" });
  expect(formatDb(0)).toEqual({ text: "0", spoken: "0 decibels" });
  expect(formatDb(-2)).toEqual({ text: "-2", spoken: "minus 2 decibels" });
  expect(formatDb(-12).text).toBe("-12");
});
