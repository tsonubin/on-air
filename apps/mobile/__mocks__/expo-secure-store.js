const values = new Map();

module.exports = {
  WHEN_UNLOCKED_THIS_DEVICE_ONLY: "WHEN_UNLOCKED_THIS_DEVICE_ONLY",
  getItemAsync: jest.fn(async (key) => values.get(key) ?? null),
  setItemAsync: jest.fn(async (key, value) => {
    values.set(key, value);
  }),
  deleteItemAsync: jest.fn(async (key) => {
    values.delete(key);
  }),
  __reset: () => values.clear(),
};
