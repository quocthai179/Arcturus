// jsdom không có layout nên không cài `scrollIntoView`; test nào cần kiểm thì tự thay bằng mock.
Element.prototype.scrollIntoView = () => {};
