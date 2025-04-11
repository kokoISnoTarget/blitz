
function nodeListIterator(nodeList) {
  let index = 0;
  return {
    next: function () {
      if (index < nodeList.length) {
        return { done: false, value: [index, nodeList[index++]] };
      } else {
        return { done: true };
      }
    },
    [Symbol.iterator]: function () {
      return this;
    },
  };
}

globalThis.__internal_nodeListIterator = nodeListIterator;
