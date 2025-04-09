
function nodeListIterator(nodeList) {
  let index = 0;
  return {
    next: function () {
      if (index < nodeList.length) {
        return { value: nodeList[index++], done: false };
      } else {
        return { done: true };
      }
    }
  };
}

globalThis.__internal_nodeListIterator = nodeListIterator;
