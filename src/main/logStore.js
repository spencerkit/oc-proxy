const { EventEmitter } = require("node:events");

class LogStore extends EventEmitter {
  constructor(limit = 100) {
    super();
    this.limit = limit;
    this.items = [];
  }

  append(entry) {
    this.items.push({
      timestamp: new Date().toISOString(),
      ...entry
    });
    if (this.items.length > this.limit) {
      this.items.splice(0, this.items.length - this.limit);
    }
    this.emit("append", this.items[this.items.length - 1]);
  }

  list(max = 100) {
    return this.items.slice(-max);
  }

  clear() {
    this.items = [];
    this.emit("clear");
  }
}

module.exports = {
  LogStore
};
